//! The Loren CLI on its own, without building the editor: `cargo run -p
//! rbx_loren --example loren -- init my-game`. The editor exposes the same
//! entry point as `rbxstudio loren ...`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(rbx_loren::cli::main(
        &args,
        rbx_loren::cli::Overrides::default(),
    ));
}
