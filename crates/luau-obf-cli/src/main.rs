use clap::Parser;
use luau_obf::{obfuscate, EnvBinding, Options};
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(name = "luau-obf", version, about = "Luau obfuscator")]
struct Args {
    /// Input .luau file
    input: PathBuf,
    /// Output file
    #[arg(short = 'o', long = "output")]
    output: PathBuf,
    /// 64-hex-character seed (32 bytes). If omitted, a random seed is used.
    #[arg(long = "seed")]
    seed: Option<String>,
    /// Suppress progress on stderr.
    #[arg(long = "quiet")]
    quiet: bool,
    /// Luau expression evaluated at runtime as the environment binding key
    /// contribution (e.g. `tostring(game.PlaceId)`). Must be paired with
    /// --env-bind-expected.
    #[arg(long = "env-bind-expr", requires = "env_bind_expected")]
    env_bind_expr: Option<String>,
    /// The string value the obfuscator commits to at build time. Must be
    /// paired with --env-bind-expr.
    #[arg(long = "env-bind-expected", requires = "env_bind_expr")]
    env_bind_expected: Option<String>,
}

fn parse_seed(s: &str) -> Result<[u8; 32], String> {
    if s.len() != 64 {
        return Err(format!("seed must be 64 hex chars, got {}", s.len()));
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
            .map_err(|_| format!("invalid hex at position {}", i * 2))?;
    }
    Ok(out)
}

fn hex_of(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn main() -> ExitCode {
    let args = Args::parse();
    let source = match fs::read_to_string(&args.input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{{\"error\":\"read input\",\"message\":\"{}\"}}", e);
            return ExitCode::from(1);
        }
    };
    let seed = match args.seed.as_deref().map(parse_seed) {
        None => None,
        Some(Ok(s)) => Some(s),
        Some(Err(msg)) => {
            eprintln!("{{\"error\":\"seed\",\"message\":\"{}\"}}", msg);
            return ExitCode::from(1);
        }
    };
    let env_binding = match (args.env_bind_expr, args.env_bind_expected) {
        (Some(expr), Some(expected)) => Some(EnvBinding { runtime_expr: expr, expected_value: expected }),
        _ => None,
    };
    let opts = Options { seed, env_binding };
    match obfuscate(&source, opts) {
        Ok(result) => {
            if !args.quiet {
                eprintln!("seed={}", hex_of(&result.seed_used));
            }
            if let Err(e) = fs::write(&args.output, &result.output) {
                eprintln!("{{\"error\":\"write output\",\"message\":\"{}\"}}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            let exit_code = match &e {
                luau_obf::Error::Parse(_) | luau_obf::Error::Hir(_) => 1,
                _ => 2,
            };
            eprintln!("{{\"error\":\"obfuscate\",\"message\":\"{}\"}}", e);
            ExitCode::from(exit_code)
        }
    }
}
