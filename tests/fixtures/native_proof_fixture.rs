//! Scratch test fixture. No networking, credentials, publication or real CI.
use std::{
    env, fs,
    io::{self, Write},
    process::{Command, Stdio},
    thread,
    time::Duration,
};
#[allow(clippy::zombie_processes)] // Intentional fixture: the test supervisor owns/reaps descendants.
fn main() {
    let args: Vec<_> = env::args().collect();
    match args[1].as_str() {
        "ok" => {
            println!("checked");
            eprintln!("diagnostic");
        }
        "exit" => {
            eprintln!("fixture failed");
            std::process::exit(args[2].parse().unwrap());
        }
        "artifact" => {
            fs::write(&args[2], args[3].as_bytes()).unwrap();
            println!("built actual artifact");
        }
        "sleep" => {
            if let Some(path) = args.get(3) {
                fs::write(path, std::process::id().to_string()).unwrap();
            }
            thread::sleep(Duration::from_millis(args[2].parse().unwrap()));
        }
        "noisy" => {
            let buf = vec![b'x'; 8192];
            loop {
                io::stdout().write_all(&buf).unwrap();
                io::stderr().write_all(&buf).unwrap();
            }
        }
        "mutate" => {
            fs::write(&args[2], "modified during check").unwrap();
        }
        "environment" => {
            assert!(env::var_os("HOME").is_none());
            assert!(env::var_os("AWS_ACCESS_KEY_ID").is_none());
            assert!(env::var_os("DECAPOD_NATIVE_PROOF_ACTIVE").is_some());
            println!("{}", args[2]);
        }
        "descendant" => {
            let child = Command::new(env::current_exe().unwrap())
                .args(["sleep", "30000", &args[2]])
                .stdin(Stdio::null())
                .spawn()
                .unwrap();
            // This mode deliberately leaves an ordinary child holding stdout.
            // It is killed with the owned process group by the supervisor.
            fs::write(&args[3], child.id().to_string()).unwrap();
            if args.get(4).is_some_and(|s| s == "wait") {
                thread::sleep(Duration::from_secs(30));
            }
        }
        #[cfg(unix)]
        "symlink-artifact" => {
            std::os::unix::fs::symlink(&args[3], &args[2]).unwrap();
        }
        other => panic!("unknown fixture action: {other}"),
    }
}
