fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("icono.ico");
        // Metadata del ejecutable
        res.set("ProductName", "UpdateGit");
        res.set("FileDescription", "Actualizador desde GitHub Releases");
        res.set("LegalCopyright", "© 2025 AMG");
        res.set("CompanyName", "AMG");
        res.set("FileVersion", env!("CARGO_PKG_VERSION"));
        res.set("ProductVersion", env!("CARGO_PKG_VERSION"));
        res.compile().expect("Error al compilar recursos Windows");
    }
}
