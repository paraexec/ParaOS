use std::env;

fn main() -> std::io::Result<()> {
    let path = env::current_dir()?;
    println!(
        "cargo:rustc-link-arg=-T{}/bootboot.ld",
        path.to_str().unwrap()
    );
    Ok(())
}
