#![forbid(unsafe_code)]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = std::io::stdout();
    let code = craft_corpus::cli::run(&args, &mut out);
    std::process::exit(code);
}
