fn main() {
    if let Err(error) = decapod::run() {
        if let decapod::core::error::DecapodError::DiagnosticExit(code) = error {
            std::process::exit(code);
        }
        eprintln!("{error}");
        std::process::exit(1);
    }
}
