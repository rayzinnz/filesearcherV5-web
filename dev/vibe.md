https://aider.chat/docs/more/edit-formats.html

```sh
cd /home/ray/MEGA/Rays/Programming/rust/filesearcherV5-web
aider --model openai/llm --chat-mode ask
aider --model openai/llm --lint-cmd "sh -c 'cargo check --message-format=short'" --auto-lint --edit-format diff --yes-always


/add src/main.rs src/lib.rs Cargo.toml config.toml
/add src/lib.rs Cargo.toml

/run cargo check --message-format=short
/run cargo build
```

**vibe**
This is a web server written in rust using axum. It is for uploading and downloading files.

Add a new route "delete_file" which will take a sub_dir filepath (including filename), and on this server delete the file at base_dir+sub_dir(including filename), if it exists.
