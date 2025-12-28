use std::io::Write;

pub type Output = dyn Write + Send + Sync + 'static;
