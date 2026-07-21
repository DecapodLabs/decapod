fn main() {
    if let Err(error) = decapod::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
