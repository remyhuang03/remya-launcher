fn main() -> std::io::Result<()> {
    rustix::process::umask(rustix::fs::Mode::from_bits_retain(0o077));
    remya_launcher_calculator::serve(std::io::stdin(), std::io::stdout())
}
