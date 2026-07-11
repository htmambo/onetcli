fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../resources/windows/omnihub.ico");
        res.set("ProductName", "OmniHub");
        res.set("FileDescription", "OmniHub - Database, SSH, Terminal, AI Tools");
        res.set("LegalCopyright", "Copyright (c) 2025 OmniHub");
        res.compile().expect("Failed to compile Windows resources");
    }
}
