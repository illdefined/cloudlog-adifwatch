#![feature(
	maybe_uninit_slice,
)]

#[macro_use]
extern crate lazy_static;

use std::env;
use std::fs::File;
use std::io::{self, BufReader};
use std::io::prelude::*;
use std::mem::MaybeUninit;
use std::option::Option;
use std::path::Path;
use std::process::exit;
use std::result::Result;
use std::str;
use std::string::String;
use std::sync::mpsc::channel;

use notify::{Watcher, RecursiveMode, Event, EventKind, event::ModifyKind};
use regex::bytes::Regex;
use ureq::Agent;
use url::Url;

/// ADIF records reader
struct RecordsReader {
	file: File,
	buffer: Vec<u8>
}

impl RecordsReader {
	/// Read chunk size
	const CHUNK_SIZE: usize = 256 * 1024;

	/// Create new records reader
	fn new(file: File) -> Self {
		Self {
			file,
			buffer: Vec::<u8>::new()
		}
	}

	/// Length of longest chunk of complete records in the buffer
	fn complete(&self) -> usize {
		lazy_static! {
			static ref RE: Regex = Regex::new(r"(?is-u).*<eor>[\r\n]*").unwrap();
		}

		// Find last complete record
		match RE.find_iter(&self.buffer).last() {
			Some(m) => m.end(),
			None => 0
		}
	}

}

impl Iterator for RecordsReader {
	type Item = String;

	/// Read a chunk of complete ADIF records
	fn next(&mut self) -> Option<String> {
		self.buffer.reserve(Self::CHUNK_SIZE);
		let tail = unsafe { MaybeUninit::slice_assume_init_mut(self.buffer.spare_capacity_mut()) };
		let rlen = self.file.read(tail).unwrap_or_else(|err| {
			eprintln!("Failed to read from log file: {}", err);
			exit(74);
		});

		unsafe { self.buffer.set_len(self.buffer.len() + rlen); }
		let clen = self.complete();

		if clen == 0 {
			None
		} else {
			let rec = String::from(str::from_utf8(&self.buffer[..clen]).unwrap_or_else(|err| {
				eprintln!("<2>Unable to parse chunk as UTF-8: {}", err);
				exit(65);
			}));

			// Move remaining items to the front
			self.buffer.drain(..clen);

			Some(rec)
		}
	}
}

/// Upload new records from log
fn upload(agent: &mut ureq::Agent, url: &Url, key: &str, profile: &str, log: &mut RecordsReader) {
	for rec in log {
		agent.request_url("PUT", url)
		     .set("User-Agent", concat!(env!("CARGO_PKG_NAME"),
		                                "/", env!("CARGO_PKG_VERSION_MAJOR"),
		                                ".", env!("CARGO_PKG_VERSION_MINOR"),
		                                " (+", env!("CARGO_PKG_REPOSITORY"), ")"))
		     .send_json(ureq::json!({
			"key": key,
			"station_profile_id": profile,
			"type": "adif",
			"string": rec
		})).unwrap_or_else(|err| {
			eprintln!("<2>Failed to upload log records: {}", err);
			exit(74);
		});

		eprintln!("<7>Uploaded {} bytes of log data.", rec.len());
	}
}

/// Read API key from file
fn read_key(path: &str) -> io::Result<String> {
	Ok(BufReader::new(File::open(path)?).lines().next().unwrap()?.trim().to_string())
}

/// Construct QSO API URL
fn api_url(base: &str) -> Result<Url, url::ParseError> {
	Url::parse(base)?.join("api/qso")
}

fn main() -> io::Result<()> {
	let mut args = env::args();

	if args.len() <= 1 {
		eprintln!("Usage: {} [base URL] [API key file] [Profile ID] [ADIF log file]",
		          args.next().unwrap());
		exit(64);
	}

	let url = api_url(&args.nth(1).unwrap_or_else(|| {
		eprintln!("Missing CloudLog base URL");
		exit(64);
	})).unwrap_or_else(|err| {
		eprintln!("Failed to construct QSO API URL: {}", err);
		exit(64);
	});

	let key = read_key(&args.next().unwrap_or_else(|| {
		eprintln!("Missing API key file path");
		exit(64);
	})).unwrap_or_else(|err| {
		eprintln!("Failed to read API key: {}", err);
		exit(66);
	});

	let profile = args.next().unwrap_or_else(|| {
		eprintln!("Missing station profile ID");
		exit(64);
	});

	let log_path = args.next().unwrap_or_else(|| {
		eprintln!("Missing log file path");
		exit(64);
	});

	let mut log = RecordsReader::new(File::open(&log_path).unwrap_or_else(|err| {
		eprintln!("Failed to open log file: {}", err);
		exit(66);
	}));

	let (tx, rx) = channel();
	let mut watcher = notify::recommended_watcher(tx).unwrap_or_else(|err| {
		eprintln!("Failed to set up file watcher: {}", err);
		exit(71);
	});

	watcher.watch(Path::new(&log_path), RecursiveMode::NonRecursive).unwrap_or_else(|err| {
		eprintln!("Unable to watch log file for changes: {}", err);
		exit(71);
	});

	let mut agent = Agent::new();
	eprintln!("<6>Performing initial full log upload.");
	upload(&mut agent, &url, &key, &profile, &mut log);

	for ev in rx {
		#[cfg(debug_assertions)]
		eprintln!("<7>Log file event: {:?}", ev);

		match ev {
			Ok(Event { kind: EventKind::Modify(ModifyKind::Data(_)), paths: _, attrs: _ }) => {
				eprintln!("<6>Change detected in log file. Checking for updates.");
				upload(&mut agent, &url, &key, &profile, &mut log);
			},
			Ok(Event { kind: EventKind::Remove(_), paths: _, attrs: _ }) => {
				eprintln!("<2>Log file has been removed. Bailing out.");
				exit(74);
			},
			Ok(_) => { },
			Err(err) => {
				eprintln!("<2>Error detected while watching for file changes: {}", err);
				exit(71);
			}
		}
	}

	Ok(())
}
