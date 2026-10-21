//! Project the actual release manifest version into the separate worker lock.
//! Run after release-plz update, inside the owned production/derivative checkout.
//! No network, dependency resolution, Git mutation or publication occurs here.
use decapod::core::derived_release_driver::reconcile_worker_lock;

fn main() {
    if let Err(error) = run() {
        eprintln!("Native worker release projection failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> std::io::Result<()> {
    if std::env::args_os().len() != 1 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "native lock projection accepts no arguments; use its owned checkout",
        ));
    }
    let root = std::env::current_dir()?.canonicalize()?;
    let changed = reconcile_worker_lock(&root, 16 * 1024 * 1024)?;
    println!("native_worker_lock_changed={changed}");
    Ok(())
}
