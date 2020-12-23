use crate::server::{LatexmlResponse, Server};

use std::process::{Command};
use std::error::Error;
use std::result::Result;
use std::sync::Arc;
use std::path::Path;
use std::fs::{read_dir, create_dir_all};

use crossbeam::queue::ArrayQueue;
use csv::{ReaderBuilder,WriterBuilder};
use itertools::Itertools;
use rayon::prelude::*;
use which::which;

#[derive(Debug)]
pub struct Harness {
  pub cpus: u16,
  pub from_port: u16,
  pub batch_size: usize,
  servers: Arc<ArrayQueue<Server>>,
}

impl Harness {
  /// Creating a new harness will spin up as many latexmls servers as available `cpus`,
  /// starting from the specified port
  /// TODO: we need a cheap check if a server PID has died, and a reboot if so.
  /// Upon Harness `Drop`, the latexmls server processes are reaped from the OS
  pub fn new(
    from_port: u16,
    cpus: u16,
    cache_key: &str,
    boot_options: Vec<(String, String)>,
  ) -> Result<Self, Box<dyn Error>> {
    let latexmls_which = which("latexmls").expect("latexmls needs to be installed and visible");
    let latexmls_exec =latexmls_which.as_path().to_string_lossy().to_string();
    let servers = Arc::new(ArrayQueue::new(cpus.into()));
    (from_port..from_port + cpus).into_par_iter().for_each(|port| {
      servers.clone()
        .push(Server::boot_at(
          latexmls_exec.to_string(),
          port,
          cache_key.to_string(),
          boot_options.clone(),
        ).expect(&format!("failed to init first latexmls servers from port {}, check your installation.", port)))
        .expect("failed to initialize server ArrayQueue");
    });
    Ok(Harness {
      from_port,
      cpus,
      // Let's both fit in RAM and also maximally utilize the CPUs
      // without artificial round-robin bottlenecks (batch_size=cpus)
      batch_size: (100 * cpus).into(),
      servers
    })
  }

  /// Converts a (flat) directory of CSV files,
  /// each file of which is processed as per `convert_file`
  pub fn convert_dir(
    &mut self,
    input_dir: &str,
    output_dir: &str,
    log_dir: &str,
  ) -> Result<(), Box<dyn Error>> {
    // Prepare files for I/O
    let input_path = Path::new(input_dir);
    if !input_path.is_dir() || !input_path.exists() {
      return Err(format!("Harness::convert_dir should only ever be called on existing directories: {}", input_dir).into());
    };
    for read_result in read_dir(input_path)? {
      if let Ok(dir_entry) = read_result {
        let filename = dir_entry.file_name();
        let entry = filename.to_string_lossy();
        if entry.ends_with(".csv") {
          self.convert_file(&format!("{}/{}",input_dir, entry), &format!("{}/result_{}",output_dir, entry), &format!("{}/{}.log",log_dir, entry))?;
        }
      }
    }

    Ok(())
  }

  /// Converts a CSV file containing one TeX input string per line
  /// creating a CSV and log files with respective results and status codes
  /// in the same line order as the input.
  pub fn convert_file(
    &mut self,
    input_file: &str,
    output_file: &str,
    log_file: &str,
  ) -> Result<(), Box<dyn Error>> {
    if self.cpus as usize != rayon::current_num_threads() {
      // if we requested different number of CPUs, change that in rayon
      rayon::ThreadPoolBuilder::new()
        .num_threads(self.cpus.into())
        .build_global()?;
    }

    // Prepare files for I/O
    let input_path = Path::new(input_file);
    let input_dir = if input_path.is_dir() || !input_path.exists() {
      return Err(format!("Harness::convert_file should only ever be called on existing CSV files: {}", input_file).into());
    } else {
      input_path.parent().unwrap()
    };
    if !input_dir.exists() {
      create_dir_all(input_dir)?; }
    let output_path = Path::new(output_file);
    let output_dir = if output_path.is_dir() {
      output_path
    } else {
      output_path.parent().unwrap()
    };
    if !output_dir.exists() {
      create_dir_all(output_dir)?; }
    let log_path = Path::new(log_file);
    let log_dir = if log_path.is_dir() {
      log_path
    } else {
      log_path.parent().unwrap()
    };
    if !log_dir.exists() {
      create_dir_all(log_dir)?; }


    let mut reader = ReaderBuilder::new().has_headers(false).from_path(input_file)?;
    let mut out_writer = WriterBuilder::new().from_path(output_file)?;
    let mut log_writer = WriterBuilder::new().from_path(log_file)?;

    // Each line of the input file represents a separate conversion job.
    // we stream it in line by line, allocating large enough batches in RAM
    // to process in parallel
    let batched_record_iter = reader
      .records()
      .filter(|record| record.is_ok())
      .map(|record| record.unwrap())
      .chunks(self.batch_size);

    // we can't chunk in the generic function, since mapping each data item to &str is specific to the reader
    // in this case our CSV reader allows for `as_slice`, but if we were reading from e.g. json, yaml, etc
    // the path to a string slice would be different.
    //
    // Similarly we can't map to &str before we collect the chunks into a vec,
    // as Rust wants to have a solid grasp on the owned data before it allows us to borrow from it.
    let mut progress_count = 1;
    for batch in batched_record_iter.into_iter() {
      let chunk_data : Vec<_> = batch.collect();
      eprintln!("-- converting job #{}", progress_count);
      let b_len = chunk_data.len();
      progress_count += b_len;
      let results = self.convert_iterator(chunk_data.iter().map(|x| x.as_slice()));
      // We must always ensure we match inputs with outputs, or large streams become corrupted
      let r_len = results.len();
      assert_eq!(r_len, b_len, "panic: we got {} results for {} inputs!",r_len, b_len);

      // Flush this batch to output files
      for response in results.into_iter() {
        out_writer.write_record(&[response.result])?;
        log_writer.write_record(&[response.status_code.to_string()])?;
      }
      out_writer.flush()?;
      log_writer.flush()?;
    }
    Ok(())
  }

  /// Convert all jobs *from* a blocking serial iterator,
  /// bridging to parallel latexmls servers via rayon.
  /// Output is returned in the same order as the input entries.
  /// Note that you may need to batch your data before using this method,
  /// as all output values are held in memory at the moment
  fn convert_iterator<'a, I>(&mut self, vals: I) -> Vec<LatexmlResponse>
  where
      I: Iterator<Item = &'a str>+Send,
  {
    let mut results : Vec<_> = vals.enumerate().par_bridge().map(|(index, record)| {
      let mut server = self.servers.pop().unwrap();
      let mut result = server.convert(record);
      if result.is_err() { // retry 1
        result = server.convert(record);
      }
      if result.is_err() { // retry 2
        result = server.convert(record);
      }
      let response = match result {
        Ok(r) => r,
        Err(_) => {
          LatexmlResponse::default()
        }
      };
      self
      .servers
      .push(server)
      .unwrap();
      (index,response)
    }).collect();
    results.sort_by_key(|x| x.0);
    results.into_iter().map(|x| x.1).collect()
  }


  pub fn convert_one(&mut self, job: &str) -> Result<String, Box<dyn Error>> {
    // select an available server
    let mut server = self.servers.pop().unwrap();
    // convert
    let payload = server.convert(job)?;
    // make server available again
    self
      .servers
      .push(server)
      .map_err(|_e| "failed to recycle server")?;

    Ok(payload.result)
  }
}

// impl Drop for Harness {
//   fn drop(&mut self) {
//     while let Some(server) = self.servers.pop() {
//       drop(server);
//     }
//     let _child = Command::new("killall")
//       .arg("latexmls")
//       .spawn().unwrap().wait();
//   }
// }