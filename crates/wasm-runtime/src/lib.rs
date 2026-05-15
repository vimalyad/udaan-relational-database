//! WASM runtime bindings. Phase 9 placeholder.
//! wasm-bindgen exports will be added here.

// Suppress wasm-bindgen unused import warnings during native builds
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
