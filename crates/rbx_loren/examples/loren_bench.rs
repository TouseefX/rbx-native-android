//! Times Loren's runtime hot paths in the editor's Luau sandbox and prints a
//! table. Usage: `cargo run --release -p rbx_loren --example loren_bench [case filter]`.

use std::path::Path;
use std::process::ExitCode;

use rbx_loren::place::insert_path;
use rbx_loren::specs::{loren_unit_place, run_script};

fn lua_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            ' '..='~' => out.push(c),
            _ => {}
        }
    }
    out.push('"');
    out
}

fn run(filter: Option<String>) -> Result<String, String> {
    let mut dom = loren_unit_place()?;
    let storage = dom
        .root_refs()
        .iter()
        .copied()
        .find(|r| dom.get(*r).is_some_and(|i| i.class() == "ServerStorage"))
        .ok_or("the place has no ServerStorage")?;
    let bench = Path::new(env!("CARGO_MANIFEST_DIR")).join("bench/LorenBench.luau");
    insert_path(&mut dom, storage, "LorenBench", &bench)?;
    let filter = filter.map_or_else(|| "nil".to_string(), |f| lua_string(&f));
    let source = format!(
        "require(game:GetService(\"ServerStorage\").LorenBench)({{ filter = {filter} }})"
    );
    run_script(dom, &source)
}

fn main() -> ExitCode {
    let output = match run(std::env::args().nth(1)) {
        Ok(output) => output,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };
    let rows: Vec<Vec<&str>> = output
        .lines()
        .filter_map(|line| line.strip_prefix("BENCH|"))
        .map(|rest| rest.split('|').collect())
        .collect();
    println!("case                  median us       min us       max us");
    for row in &rows {
        match row.as_slice() {
            ["runtime", version] => println!("Loren {version}"),
            [name, mid, lo, hi] => println!("{name:<18} {mid:>12} {lo:>12} {hi:>12}"),
            _ => {}
        }
    }
    if rows.len() < 2 {
        eprintln!("{output}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
