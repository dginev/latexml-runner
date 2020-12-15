use which::which;

fn main() {
  if which("latexmls").is_err() {
    panic!("Could not find the latexmls executable in PATH. See https://github.com/dginev/LaTeXML-Plugin-latexmls");
  }
}