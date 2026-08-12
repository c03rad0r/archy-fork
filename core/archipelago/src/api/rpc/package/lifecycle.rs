// Container lifecycle operations.
//
// Split into focused sub-modules:
//   - install.rs      — Image pulling, container creation, volume setup, multi-container stacks
//   - runtime.rs      — Start, stop, restart, uninstall operations
//   - dependencies.rs — Dependency resolution, startup ordering, network requirements
//
// All public handler methods (handle_package_*) are implemented on RpcHandler
// in their respective sub-modules and remain callable from the RPC dispatcher.
