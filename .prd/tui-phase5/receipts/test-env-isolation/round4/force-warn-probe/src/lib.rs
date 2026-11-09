#![allow(clippy::all)]
#![allow(warnings)]

#[macro_export]
macro_rules! process_var {
    ($name:literal) => {
        match () {
            #[allow(clippy::disallowed_methods, reason = "env: macro")]
            () => ::std::env::var_os($name),
        }
    };
}

pub fn via_macro() -> Option<std::ffi::OsString> {
    process_var!("PATH")
}

#[allow(clippy::style)]
pub fn group_allow() -> bool {
    std::env::var_os("A").is_some()
}

#[expect(clippy::disallowed_methods)]
pub fn expected() -> bool {
    std::env::var_os("B").is_some()
}

#[cfg_attr(all(), allow(clippy::disallowed_methods))]
pub fn cfg_attr_allow() -> bool {
    std::env::var("C").is_ok()
}

pub fn path_reference() -> bool {
    let f = std::env::var_os;
    f("D").is_some()
}

pub fn alias() -> bool {
    use std::env::var_os as v;
    v("E").is_some()
}
