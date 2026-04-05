use std::io::stdin;
use std::io::Read;

fn main() {
  println!("Hello from B!");
  let mut input = String::new();
  stdin().read_to_string(&mut input).unwrap();
  println!("Stdin: {}", input);
}
