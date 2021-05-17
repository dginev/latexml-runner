use latexml_runner::Harness;
use rand::prelude::*;
use std::time::Instant;

#[test]
fn convert_file() {
  let start_test = Instant::now();
  let from_port: u16 = thread_rng().gen_range(11000, 13000);
  let harness_result = Harness::new(
    from_port,
    0,
    [
      ("whatsin", "math"),
      ("whatsout", "math"),
      ("preload", "article.cls"),
      ("preload", "amsmath.sty"),
    ]
    .iter()
    .map(|(x, y)| (x.to_string(), y.to_string()))
    .collect(),
  );
  assert!(harness_result.is_ok(), "{:?}", harness_result);
  let mut harness = harness_result.unwrap();
  let result = harness.convert_dir(
    "tests/data/sample_dir",
    "tests/scratch/sample_dir",
    "tests/scratch/sample_dir",
  );
  assert!(result.is_ok(), "{:?}", result);
  eprintln!(
    "tests/data directory test took {:?}ms",
    start_test.elapsed().as_millis()
  );
}
