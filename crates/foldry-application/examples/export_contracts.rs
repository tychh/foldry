use std::{env, fs, path::PathBuf, process::ExitCode};

use foldry_application::typescript_bindings;

fn main() -> ExitCode {
    let check = env::args().skip(1).any(|argument| argument == "--check");
    let output = output_path();
    let generated = typescript_bindings();

    if check {
        return match fs::read_to_string(&output) {
            Ok(committed) if committed == generated => ExitCode::SUCCESS,
            Ok(_) => {
                eprintln!(
                    "TypeScript contracts are stale. Run `pnpm contracts:generate` and commit {}",
                    output.display()
                );
                ExitCode::FAILURE
            }
            Err(error) => {
                eprintln!(
                    "Cannot read generated contracts {}: {error}",
                    output.display()
                );
                ExitCode::FAILURE
            }
        };
    }

    if let Some(parent) = output.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        eprintln!("Cannot create {}: {error}", parent.display());
        return ExitCode::FAILURE;
    }
    match fs::write(&output, generated) {
        Ok(()) => {
            println!("Generated {}", output.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Cannot write {}: {error}", output.display());
            ExitCode::FAILURE
        }
    }
}

fn output_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../frontend/src/shared/contracts/generated.ts")
}
