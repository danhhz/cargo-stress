// Copyright 2019 Daniel Harrison. All Rights Reserved.

use std::env;
use std::error;
use std::io;
use std::io::Write;
use std::path;
use std::process;
use std::sync;
use std::sync::atomic;
use std::sync::mpsc;
use std::thread;
use std::time;

use serde::Deserialize;
use serde_json;

#[derive(Deserialize, Debug)]
struct BuildMessage {
    executable: Option<path::PathBuf>,
}

fn main() {
    let args = env::args().collect::<Vec<_>>();
    if let Err(err) = run(parse_args(&args)) {
        eprintln!("failed: {}", err);
        process::exit(1)
    };
}

fn run(args: ParsedArgs) -> Result<(), Box<dyn error::Error>> {
    let mut build_args = vec!["test", "--no-run", "--message-format=json"];
    for arg in &args.cargo_args {
        build_args.push(arg);
    }
    println!("Compiling test binaries: cargo {}", build_args.join(" "));
    // Compile the test binaries, printing the stderr so the user sees compilation
    // progress.
    let build_cmd = process::Command::new("cargo")
        .args(&build_args)
        .stderr(process::Stdio::inherit())
        .output()?;
    if !build_cmd.status.success() {
        io::stderr()
            .write_all(&build_cmd.stdout)
            .expect("copying build stdout to stderr");
        // There will already be an error printed.
        process::exit(1);
    }
    let test_binaries = std::str::from_utf8(&build_cmd.stdout)?
        .lines()
        .map(serde_json::from_str::<BuildMessage>)
        .collect::<Result<Vec<BuildMessage>, _>>()?
        .iter()
        .flat_map(|x| x.executable.clone())
        .collect::<Vec<_>>();

    for test_binary in &test_binaries {
        println!(
            "Running test binary: {:?} {}",
            test_binary,
            args.test_args.join(" ")
        );
    }
    let start = time::Instant::now();
    let (run_results_tx, run_results_rx) = mpsc::sync_channel(1);
    const PARALLELISM: usize = 8;
    let _workers = (0..PARALLELISM)
        .map(|_| {
            let (test_binaries, test_args) = (test_binaries.clone(), args.test_args.clone());
            let run_results_tx = run_results_tx.clone();
            thread::spawn(move || worker(test_binaries, test_args, run_results_tx))
        })
        .collect::<Vec<_>>();

    let runs = sync::Arc::new(atomic::AtomicUsize::new(0));
    let failures = sync::Arc::new(atomic::AtomicUsize::new(0));
    let _progress = {
        let (start, runs, failures) = (start.clone(), runs.clone(), failures.clone());
        thread::spawn(move || progress(start, runs, failures))
    };

    const MAX_FAILURES: usize = 1;
    loop {
        let result = run_results_rx.recv()?;
        let runs = runs.fetch_add(1, atomic::Ordering::SeqCst) + 1;
        let failures = if result.len() > 0 {
            failures.fetch_add(1, atomic::Ordering::SeqCst) + 1
        } else {
            failures.load(atomic::Ordering::SeqCst)
        };

        // Print at the beginning of the first run so we know it's going.
        if runs == 1 {
            println!(
                "{} runs so far, {} failures, over {}s",
                runs,
                failures,
                start.elapsed().as_secs()
            );
        };

        if failures >= MAX_FAILURES {
            eprintln!(
                "{} runs completed, {} failures, over {}s",
                runs,
                failures,
                start.elapsed().as_secs()
            );
            io::stderr()
                .write_all(&result)
                .expect("writing test failure to stderr");
            process::exit(2);
        }
    }
}

struct ParsedArgs {
    cargo_args: Vec<String>,
    test_args: Vec<String>,
}

fn parse_args<T: AsRef<str>>(args: &[T]) -> ParsedArgs {
    let mut parsed = ParsedArgs {
        cargo_args: vec![],
        test_args: vec![],
    };
    let mut i = 0;
    // Skip past the binary
    i += 1;
    // If invoked as `cargo stress`, skip over the stress.
    if let Some(arg) = args.get(i) {
        if arg.as_ref() == "stress" {
            i += 1;
        }
    }
    while let Some(arg) = args.get(i) {
        i += 1;
        if arg.as_ref() == "--" {
            break;
        }
        if arg.as_ref().starts_with("-") {
            parsed.cargo_args.push(arg.as_ref().to_string());
        } else {
            parsed.test_args.push(arg.as_ref().to_string());
        }
    }
    if i < args.len() {
        for arg in &args[i..] {
            parsed.test_args.push(arg.as_ref().to_string());
        }
    }
    parsed
}

fn worker(
    test_binaries: Vec<path::PathBuf>,
    test_args: Vec<String>,
    results_tx: mpsc::SyncSender<Vec<u8>>,
) {
    loop {
        for test_bin in &test_binaries {
            let test_cmd = process::Command::new(test_bin).args(&test_args).output();
            let send = match test_cmd {
                Err(err) => results_tx.send(err.to_string().into_bytes()),
                Ok(output) => {
                    if output.status.success() {
                        results_tx.send("".as_bytes().to_vec())
                    } else {
                        // TODO(dan): Return combined stdout + stderr.
                        results_tx.send(output.stdout)
                    }
                }
            };
            send.expect("failed to send test binary results over channel");
        }
    }
}

fn progress(
    start: time::Instant,
    runs: sync::Arc<atomic::AtomicUsize>,
    failures: sync::Arc<atomic::AtomicUsize>,
) {
    const PROGRESS_INTERVAL: time::Duration = time::Duration::from_secs(5);
    loop {
        thread::sleep(PROGRESS_INTERVAL);
        println!(
            "{} runs so far, {} failures, over {}s",
            runs.load(atomic::Ordering::SeqCst),
            failures.load(atomic::Ordering::SeqCst),
            start.elapsed().as_secs(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_args() {
        {
            let parsed = parse_args(&["cargo", "stress"]);
            assert_eq!(parsed.cargo_args, [] as [String; 0]);
            assert_eq!(parsed.test_args, [] as [String; 0]);
        }
        {
            let parsed = parse_args(&["cargo", "stress", "--foo"]);
            assert_eq!(parsed.cargo_args, ["--foo"]);
            assert_eq!(parsed.test_args, [] as [String; 0]);
        }
        {
            let parsed = parse_args(&["cargo", "stress", "--foo", "bar", "--"]);
            assert_eq!(parsed.cargo_args, ["--foo"]);
            assert_eq!(parsed.test_args, ["bar"]);
        }
        {
            let parsed = parse_args(&["cargo", "stress", "--", "--bar"]);
            assert_eq!(parsed.cargo_args, [] as [String; 0]);
            assert_eq!(parsed.test_args, ["--bar"]);
        }
        {
            let parsed = parse_args(&[
                "cargo", "stress", "--foo", "bar", "--", "baz", "--qux", "--",
            ]);
            assert_eq!(parsed.cargo_args, ["--foo"]);
            assert_eq!(parsed.test_args, ["bar", "baz", "--qux", "--"]);
        }
    }

    #[test]
    #[ignore]
    fn test_flaky() {
        let millis = time::SystemTime::now()
            .duration_since(time::UNIX_EPOCH)
            .expect("clock jumped backward")
            .as_millis();
        if millis % 1000 == 0 {
            panic!("boom");
        }
    }
}
