//! Générateur des liaisons Kotlin et Swift de `tallylight-ffi` (voir `docs/INFRA.md`).
//! Dans un crate à part : la bibliothèque embarquée ne doit pas dépendre du générateur.
fn main() {
    uniffi::uniffi_bindgen_main()
}
