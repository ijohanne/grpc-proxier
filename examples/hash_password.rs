use argon2::Argon2;
use argon2::password_hash::{PasswordHasher, SaltString, rand_core::OsRng};

fn main() {
    let password = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: cargo run --example hash_password -- <PASSWORD>");
        std::process::exit(1);
    });

    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("failed to hash password");

    println!("{hash}");
}
