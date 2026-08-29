```sh
cd /home/ray/MEGA/Rays/Programming/rust/filesearcherV5-web
aider --model openai/llm --chat-mode ask
aider --model openai/llm --lint-cmd "sh -c 'cargo check --message-format=short'" --auto-lint --yes-always


/add src/main.rs src/lib.rs Cargo.toml config.toml
/add src/lib.rs Cargo.toml

/run cargo check --message-format=short
/run cargo build
```

**vibe**
This is a web server written in rust using axum. It is for uploading and downloading files.

Add a new route to '/get_file_db'.
This will send to the http client the file at `file_db_path` in config.toml.
