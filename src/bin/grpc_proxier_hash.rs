use argon2::Argon2;
use argon2::password_hash::{PasswordHasher, SaltString, rand_core::OsRng};
use std::io::BufRead;

fn main() {
    let password = std::io::stdin()
        .lock()
        .lines()
        .next()
        .unwrap_or_else(|| {
            eprintln!("Error: no input on stdin");
            std::process::exit(1);
        })
        .unwrap_or_else(|e| {
            eprintln!("Error reading stdin: {e}");
            std::process::exit(1);
        });

    if password.is_empty() {
        eprintln!("Error: empty password");
        std::process::exit(1);
    }

    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("failed to hash password");

    println!("{hash}");
}
