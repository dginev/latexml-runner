use rand::prelude::*;
use serde::Deserialize;
use std::error::Error;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddrV4, TcpListener, TcpStream};
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
        if let Some(stream) = self.connection.take() {
          stream.shutdown(Shutdown::Both)?;
        }
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
    let addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), self.port);
    let port_is_open = { TcpListener::bind(addr).is_ok() };
    if port_is_open {
      // before we start a new latexmls process, make sure we terminate any remaining ones
      if let Some(mut child) = self.child_proc.take() {
        if let Some(stream) = self.connection.take() {
          stream.shutdown(Shutdown::Both)?;
        }
        child.kill()?;
        child.wait()?;
      }
      {
        let child = Command::new(&self.latexmls_exec)
          .arg("--port")
          .arg(&self.port.to_string())
          .arg("--autoflush")
          .arg("0")
          .arg("--timeout")
          .arg("120")
          .arg("--expire")
          .arg("4")
          .spawn()?;
        self.child_proc = Some(child);

        let a_second = time::Duration::from_millis(1000);
        thread::sleep(a_second);

        self.init_call()?;
      }
    }
    Ok(())
  }

  /// Rotates to the backup port, and resets connection and counters
  pub fn rotate_ports(&mut self) -> Result<(), Box<dyn Error>> {
    let new_backup = self.port;
    self.port = self.backup_port;
    self.backup_port = new_backup;
    self.call_count = 0;
    if let Some(mut proc) = self.child_proc.take() {
      // First check if the current process has been reaped (e.g. due to --expire)
      if let Ok(Some(_)) = proc.try_wait() {
        self.child_proc = None;
      } else {
        // If process is still around, signal to terminate it
        if let Some(stream) = self.connection.take() {
          stream.shutdown(Shutdown::Both)?;
        }
        proc.kill()?;
        proc.wait()?;
      }
    }
    Ok(())
  }

  /// Resamples ports, as latexmls is still not stable enough, and may need to be completely abandoned.
  /// Won't be done by the Harness, but some external applications may find it useful.
  pub fn resample_ports(&mut self, from: u16, to: u16) -> Result<(), Box<dyn Error>> {
    let new_port: u16 = thread_rng().gen_range(from, to);
    let new_backup = new_port + 200;
    self.port = new_port;
    self.backup_port = new_backup;
    if let Some(mut proc) = self.child_proc.take() {
      if let Ok(Some(_)) = proc.try_wait() {
        self.child_proc = None;
      } else {
        if let Some(stream) = self.connection.take() {
          stream.shutdown(Shutdown::Both)?;
        }
        proc.kill()?;
        proc.wait()?;
      }
    }
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
          Err(_) => {
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
    let request = format!(
      "POST 127.0.0.1:{} HTTP/1.0
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
    stream.read_to_end(&mut response_u8)?;
    if response_u8.is_empty() {
      return if allow_retry {
        self.call_latexmls(body, false)
      } else {
        Err("response was empty.".into())
      };
    }
    let body_index = find_subsequence(&response_u8, "\r\n\r\n".as_bytes()).unwrap_or(0);
    let body_u8 = &response_u8[body_index..];
    // We need to assemble our own UTF-16 string, or glyphs such as π get garbled on follow-up IO
    let payload: LatexmlResponse = serde_json::from_slice(body_u8).unwrap_or_default();
    // reuse the stream
    self.connection = Some(stream);
    Ok(payload)
  }
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
  haystack
    .windows(needle.len())
    .position(|window| window == needle)
}

impl Drop for Server {
  fn drop(&mut self) {
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
