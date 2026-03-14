macro_rules! cyan_bold {
    ($s:expr) => {
        const_format::formatcp!("\x1b[1;36m{}\x1b[0m", $s)
    };
}

pub const ABOUT: &str = cyan_bold!("Circuit Breaker Labs CLI");

pub const LONG_ABOUT: &str = const_format::formatcp!(
    "{} {}

https://github.com/circuitbreakerlabs/cli
Protocol version {}",
    cyan_bold!("Circuit Breaker Labs CLI"),
    cyan_bold!(const_format::formatcp!("v{}", env!("CARGO_PKG_VERSION"))),
    crate::consts::version::PROTOCOL_VERSION
);
