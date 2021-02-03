use std::error::Error;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddrV4, TcpListener, TcpStream};
use std::process::{Child, Command};
use std::result::Result;
use std::{thread, time};

use serde::Deserialize;
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
pub(crate) struct Server {
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
      backup_port: port+200,
      cache_key,
      boot_options,
      autoflush,
      call_count: 0,
      connection: None,
      child_proc: None,
    };
    let addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), port);
    {
      TcpListener::bind(addr).expect(&format!(
        "Failed to bind on latexmls port {}, harness can't initialize!",
        port
      ));
    }

    server.ensure_server()?;
    Ok(server)
  }

  /// Convert a single job with a dedicated latexmls server, pinned to a port
  pub fn convert(&mut self, job: &str) -> Result<LatexmlResponse, Box<dyn Error>> {
    self.ensure_server()?;
    match self.call_latexmls(&format!(
      "cache_key={}&source=literal:{}",
      self.cache_key,
      encode(job)
    )) {
      Ok(r) => Ok(r),
      Err(e) => {
        // close connection on error.
        self.connection = None;
        Err(e)
      },
    }
  }

  /// Ensuring a daemonized server exists is a little more complicated than it may appear
  /// as latexmls keeps shape-shifting to different PIDs as to remain alive & healthy.
  /// The only resourceful choice is to see if the port is open & available for bind
  /// in which case we should be booting a server at it.
  pub fn ensure_server(&mut self) -> Result<(), Box<dyn Error>> {
    if self.autoflush>0 && self.call_count > self.autoflush {
      // if autoflush was breached, rotate ports.
      let new_backup = self.port;
      self.port = self.backup_port;
      self.backup_port = new_backup;
      self.connection = None;
      self.child_proc = None;
      self.call_count = 0;
    }
    let addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), self.port);
    let port_is_open = { TcpListener::bind(addr).is_ok() };
    if port_is_open {
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

  fn init_call(&mut self) -> Result<(), Box<dyn Error>> {
    // send an initialization call to the server
    let body = format!("cache_key={}&source=literal:1&", self.cache_key)
      + &self
        .boot_options
        .iter()
        .map(|opt| if opt.1.is_empty() {
          encode(&opt.0)
        } else {
          format!("{}={}", encode(&opt.0), encode(&opt.1)) })
        .collect::<Vec<_>>()
        .join("&");
    self.call_latexmls(&body)?;
    Ok(())
  }

  fn call_latexmls(&mut self, body: &str) -> Result<LatexmlResponse, Box<dyn Error>> {
    self.call_count+=1;
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
              },
            }
          },
        }
      },
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
      return Err("response was empty.".into());
    }
    let response_str = String::from_utf8_lossy(&response_u8);
    let parts: Vec<_> = response_str.split("\r\n\r\n").collect();
    let payload: LatexmlResponse = serde_json::from_str(&parts.last().unwrap()).unwrap_or_default();
    // reuse the stream
    self.connection = Some(stream);
    Ok(payload)
  }
}

impl Drop for Server {
  fn drop(&mut self) {
    if let Some(ref mut stream) = self.connection {
      stream.shutdown(Shutdown::Both).unwrap();
    }
    if let Some(ref mut child) = self.child_proc {
      child.kill().unwrap();
      child.wait().unwrap();
    }
  }
}
