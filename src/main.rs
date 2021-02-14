#[macro_use]
extern crate lazy_static;

use std::env;
use std::fs::File;
use std::io::{self, BufReader};
use std::io::prelude::*;
use std::option::Option;
use std::result::Result;
use std::time::Duration;
use std::str;
use std::string::String;
use std::sync::mpsc::channel;

use notify::{Watcher, RecursiveMode, DebouncedEvent, watcher};
use regex::bytes::Regex;
use ureq::Agent;
use url::Url;

struct RecordsReader {
	file: File,
	buffer: Vec<u8>,
	length: usize
}

impl RecordsReader {
	fn new(file: File) -> Self {
		Self {
			file: file,
			buffer: Vec::<u8>::new(),
			length: 0
		}
	}

	fn complete(&self) -> usize {
		lazy_static! {
			static ref RE: Regex = Regex::new(r"(?is-u).*<eor>[\r\n]*").unwrap();
		}

		/* Find last complete record */
		match RE.find_iter(&self.buffer[..self.length]).last() {
			Some(m) => m.end(),
			None => 0
		}
	}

}

impl Iterator for RecordsReader {
	type Item = String;

	fn next(&mut self) -> Option<String> {
		self.buffer.resize(self.length + 8192, 0);
		let tail = &mut self.buffer[self.length..];
		let rlen = self.file.read(tail).unwrap();
		self.length += rlen;

		let clen = self.complete();

		if clen == 0 {
			None
		} else {
			let rec = String::from(str::from_utf8(&self.buffer[0..clen]).unwrap());

			/* Move remaining items to the front */
			for idx in 0..(self.length - clen) {
				self.buffer[idx] = self.buffer[clen + idx];
			}

			self.length -= clen;

			Some(rec)
		}
	}
}

fn upload(agent: &mut ureq::Agent, url: &Url, key: &str, log: &mut RecordsReader) {
	loop {
		let rec = match log.next() {
			Some(rec) => rec,
			None => break
		};

		agent.request_url("PUT", url).send_json(ureq::json!({
			"key": key,
			"type": "adif",
			"string": rec
		})).expect("Failed to upload log records");

		eprintln!("<7>Uploaded {} bytes of log data.", rec.len());
	}
}

fn read_key(path: &str) -> io::Result<String> {
	Ok(BufReader::new(File::open(&path)?).lines().next().unwrap()?.trim().to_string())
}

fn api_url(base: &str) -> Result<Url, url::ParseError> {
	Ok(Url::parse(base)?.join("/api/qso")?)
}

fn main() -> io::Result<()> {
	let mut args = env::args();

	let url = api_url(&args.nth(1).expect("Missing CloudLog base URL"))
	          .expect("Failed to generate API URL");
	let key = read_key(&args.next().expect("Missing API key file path"))
	          .expect("Failed to read API key");

	let log_path = args.next().expect("Missing log file path");
	let mut log = RecordsReader::new(File::open(&log_path).expect("Failed to open log file"));

	let mut agent = Agent::new();

	eprintln!("<6>Performing initial full log upload.");
	upload(&mut agent, &url, &key, &mut log);

	let (tx, rx) = channel();
	let mut watcher = watcher(tx, Duration::from_secs(60))
	                  .expect("Failed to set up file watcher");

	watcher.watch(&log_path, RecursiveMode::NonRecursive)
	       .expect("Unable to watch log file for changes");

	loop {
		match rx.recv().unwrap() {
			DebouncedEvent::Write(_) => {
				eprintln!("<6>Write to log detected. Performing incremental upload.");
				upload(&mut agent, &url, &key, &mut log);
			},
			_ => { }
		}
	}
}
