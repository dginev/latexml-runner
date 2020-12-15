use crate::server::{LatexmlResponse, Server};

use std::process::{Command};
use std::error::Error;
use std::result::Result;
use std::sync::Arc;
use std::path::Path;
use std::fs::create_dir_all;

use crossbeam::queue::ArrayQueue;
use csv::{ReaderBuilder,WriterBuilder};
use itertools::Itertools;
use rayon::prelude::*;
use which::which;

#[derive(Debug)]
pub struct Harness {
  pub cpus: u16,
  pub from_port: u16,
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
        ).expect("failed to init first latexmls servers, check your installation."))
        .expect("failed to initialize server ArrayQueue");
    });
    Ok(Harness {
      from_port,
      cpus,
      servers
    })
  }

  /// Multiple calls to `convert()` can be made to the same `Harness`,
  /// reusing the latexmls servers it owns.
  pub fn convert(
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
    let input_dir = if input_path.is_dir() {
      input_path
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
    // Let's both fit in RAM and also maximally utilize the CPUs.
    let batch_size = 100 * self.cpus;

    // Process each line of the input file as a separate job
    let batched_record_iter = reader
      .records()
      .filter(|record| record.is_ok())
      .map(|record| record.unwrap())
      .chunks(batch_size.into());

    for batch in batched_record_iter.into_iter() {
      let chunk_data : Vec<_> = batch.collect();
      let b_len = chunk_data.len();
      let mut results : Vec<_> =
      chunk_data.into_iter().enumerate().par_bridge().map(|(index, record)| {
        let mut server = self.servers.pop().unwrap();
        let mut result = server.convert(record.as_slice());
        if result.is_err() { // retry 1
          result = server.convert(record.as_slice());
        }
        if result.is_err() { // retry 2
          result = server.convert(record.as_slice());
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
      let r_len = results.len();

      // We must always ensure we match inputs with outputs, or large streams become corrupted
      assert_eq!(r_len, b_len, "panic: we got {} results for {} inputs!",r_len, b_len);

      // Flush this batch to output files
      let mut assert_index = 0;
      for (index,response) in results.into_iter() {
        assert_eq!(index, assert_index, "responses were out of order, expected {} got {}", assert_index, index);
        assert_index+=1;
        out_writer.write_record(&[response.result])?;
        log_writer.write_record(&[response.status_code.to_string()])?;
      }
      out_writer.flush()?;
      log_writer.flush()?;
    }
    Ok(())
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

impl Drop for Harness {
  fn drop(&mut self) {
    while let Some(server) = self.servers.pop() {
      drop(server);
    }
    let _child = Command::new("killall")
      .arg("-9").arg("latexmls")
      .spawn().unwrap().wait();
  }
}