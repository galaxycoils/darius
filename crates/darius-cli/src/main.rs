fn main() {
    if let Err(error) = darius_cli::run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
