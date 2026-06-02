//! Exposes the VM source template as a compile-time constant string so emit
//! doesn't have to do any filesystem I/O.

pub const VM_TEMPLATE: &str = include_str!("../assets/vm.luau.j2");
