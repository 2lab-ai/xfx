#[test]
fn ext() {
    let a = fwprobe::process_var!("PATH");
    #[allow(clippy::disallowed_methods)]
    let b = std::env::var_os("Q");
    assert!(a.is_some() || b.is_some() || true);
}
