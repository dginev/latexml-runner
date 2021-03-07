use rand::prelude::*;
use serde::Deserialize;
use std::error::Error;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::process::{Child, Command};
use std::result::Result;
use std::{thread, time};
use urlencoding::encode;
#[derive(Debug, Deserialize)]
pub struct LatexmlResponse {
  pub status_code: u8,
  pub status: String,
  pub result: String,
  pub log: String,
}
impl Default for LatexmlResponse {
  fn default() -> Self {
    LatexmlResponse {
      status_code: 3,
      status: String::from("Default latexml_runner fatal"),
      log: String::from("Default latexml_runner fatal"),
      result: String::new(),
    }
  }
}
impl LatexmlResponse {
  pub fn empty() -> Self {
    LatexmlResponse {
      status_code: 0,
      status: String::new(),
      log: String::new(),
      result: String::new(),
    }
  }
}

#[derive(Debug)]
pub struct Server {
  port: u16,
  backup_port: u16,
  autoflush: usize,
  call_count: usize,
  cache_key: String,
  latexmls_exec: String,
  boot_options: Vec<(String, String)>,
  child_proc: Option<Child>,
  pub connection: Option<TcpStream>,
}
impl Server {
  /// Boot a new latexmls server at a given port, with the specified options
  pub fn boot_at(
    latexmls_exec: String,
    port: u16,
    autoflush: usize,
    cache_key: String,
    boot_options: Vec<(String, String)>,
  ) -> Result<Self, Box<dyn Error>> {
    let mut server = Server {
      latexmls_exec,
      port,
      // should be a while before we have more than 200 latexmls processes on the same machine
      backup_port: port + 200,
      cache_key,
      boot_options,
      autoflush,
      call_count: 0,
      connection: None,
      child_proc: None,
    };

    server.ensure_server()?;
    Ok(server)
  }

  /// Convert a single job with a dedicated latexmls server, pinned to a port
  pub fn convert(&mut self, job: &str) -> Result<LatexmlResponse, Box<dyn Error>> {
    self.ensure_server()?;
    match self.call_latexmls(
      &format!(
        "cache_key={}&source=literal:{}",
        self.cache_key,
        encode(job)
      ),
      true,
    ) {
      Ok(r) => Ok(r),
      Err(e) => {
        // close connection on error.
        self.terminate_proc();
        Err(e)
      }
    }
  }

  /// Ensuring a daemonized server exists is a little more complicated than it may appear
  /// as latexmls keeps shape-shifting to different PIDs as to remain alive & healthy.
  /// The only resourceful choice is to see if the port is open & available for bind
  /// in which case we should be booting a server at it.
  pub fn ensure_server(&mut self) -> Result<(), Box<dyn Error>> {
    if let Some(ref mut child) = self.child_proc {
      // Check if reaped - e.g. via --expire
      // in which case we can release the pid
      if let Ok(Some(_)) = child.try_wait() {
        self.child_proc = None;
      }
    }
    if self.autoflush > 0 && self.call_count > self.autoflush {
      // if autoflush was breached, rotate ports.
      self.rotate_ports()?;
    }
    if self.child_proc.is_none() {
      let child = Command::new(&self.latexmls_exec)
        .arg("--port")
        .arg(&self.port.to_string())
        .arg("--address")
        .arg("127.0.0.1")
        .arg("--autoflush")
        .arg(&self.autoflush.to_string())
        .arg("--timeout")
        .arg("120")
        .arg("--expire")
        .arg("4")
        .spawn()?;
      self.child_proc = Some(child);

      let half_a_second = time::Duration::from_millis(500);
      thread::sleep(half_a_second);
      // Try init twice, second time a waiting little longer -
      //  to make e.g. slow CI machines succeed smoothly.
      if let Err(e) = self.init_call() {
        println!("init call for port {:?} needs to retry: {:?}", self.port, e);
        let a_second = time::Duration::from_millis(1000);
        thread::sleep(a_second);
        if let Err(e2) = self.init_call() {
          println!("init retry on port {:?} failed: {:?}", self.port, e2);
          return Err(e2);
        }
      }
    }
    Ok(())
  }

  /// Rotates to the backup port, and resets connection and counters
  pub fn rotate_ports(&mut self) -> Result<(), Box<dyn Error>> {
    eprintln!(
      "-- rotating port {} to port {}",
      self.port, self.backup_port
    );
    let new_backup = self.port;
    self.port = self.backup_port;
    self.backup_port = new_backup;
    self.call_count = 0;
    self.terminate_proc();
    Ok(())
  }

  /// Resamples ports, as latexmls is still not stable enough, and may need to be completely abandoned.
  /// Won't be done by the Harness, but some external applications may find it useful.
  pub fn resample_ports(&mut self, from: u16, to: u16) -> Result<(), Box<dyn Error>> {
    let new_port: u16 = thread_rng().gen_range(from, to);
    let new_backup = new_port + 200;
    eprintln!("-- port resampling from {} to {}.", self.port, new_port);
    self.port = new_port;
    self.backup_port = new_backup;
    self.terminate_proc();
    self.call_count = 0;
    self.ensure_server()
  }

  fn init_call(&mut self) -> Result<(), Box<dyn Error>> {
    // send an initialization call to the server
    let body = format!("cache_key={}&source=literal:1&", self.cache_key)
      + &self
        .boot_options
        .iter()
        .map(|opt| {
          if opt.1.is_empty() {
            encode(&opt.0)
          } else {
            format!("{}={}", encode(&opt.0), encode(&opt.1))
          }
        })
        .collect::<Vec<_>>()
        .join("&");
    self.call_latexmls(&body, true)?;
    Ok(())
  }

  fn call_latexmls(
    &mut self,
    body: &str,
    allow_retry: bool,
  ) -> Result<LatexmlResponse, Box<dyn Error>> {
    self.call_count += 1;
    let addr = format!("127.0.0.1:{}", self.port);
    let mut stream = match self.connection.take() {
      Some(stream) => stream,
      None => {
        // replenish the stream if needed
        match TcpStream::connect(&addr) {
          Ok(s) => s,
          Err(_e) => {
            // retry, since this can be fragile
            thread::sleep(time::Duration::from_millis(50));
            match TcpStream::connect(&addr) {
              Ok(s) => s,
              Err(e) => {
                return Err(e.into());
              }
            }
          }
        }
      }
    };
    stream.set_nodelay(true)?;
    let request = format!(
      "POST {} HTTP/1.0
Host: {}
User-Agent: latexmlc
Content-Type: application/x-www-form-urlencoded
Content-Length: {}

{}",
      addr,
      addr,
      body.len(),
      body
    );
    stream.write_all(request.as_bytes())?;
    let mut response_u8 = Vec::new();
    // Array with a fixed size
    stream.read_to_end(&mut response_u8)?;
    let body_index = find_subsequence(&response_u8, "\r\n\r\n".as_bytes()).unwrap_or(0);
    if response_u8.is_empty() || body_index == 0 {
      return if allow_retry {
        self.call_latexmls(body, false)
      } else {
        Err("response was empty.".into())
      };
    }
    let body_u8 = &response_u8[body_index + 4..];
    // We need to assemble our own UTF-16 string, or glyphs such as π get garbled on follow-up IO
    let payload: LatexmlResponse = match serde_json::from_slice(body_u8) {
      Ok(json) => json,
      Err(e) => {
        println!("-- malformed {:?}: {:?}", e, std::str::from_utf8(&body_u8));
        LatexmlResponse::default()
      }
    };
    // println!(
    //   "-- latexmls:{} returned status {} with body_size {}",
    //   self.port,
    //   payload.status_code,
    //   body_u8.len()
    // );
    // reuse the stream if we were OK
    if payload.status_code != 3 {
      self.connection = Some(stream);
    } else {
      self.connection = None;
    }
    Ok(payload)
  }
  fn terminate_proc(&mut self) {
    if let Some(ref mut stream) = self.connection {
      stream.shutdown(Shutdown::Both).unwrap();
    }
    if let Some(ref mut proc) = self.child_proc {
      if let Ok(Some(_)) = proc.try_wait() {
      } else {
        proc.kill().unwrap();
        proc.wait().unwrap();
      }
    }
  }
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
  haystack
    .windows(needle.len())
    .position(|window| window == needle)
}

impl Drop for Server {
  fn drop(&mut self) {
    self.terminate_proc()
  }
}
