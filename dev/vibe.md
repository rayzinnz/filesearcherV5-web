https://aider.chat/docs/more/edit-formats.html

```sh
cd /home/ray/MEGA/Rays/Programming/rust/filesearcherV5-web
aider --model openai/llm --chat-mode ask
aider --model openai/llm --lint-cmd "sh -c 'cargo check --message-format=short'" --auto-lint --edit-format diff
 --yes-always


/add src/main.rs src/lib.rs Cargo.toml config.toml
/add src/lib.rs Cargo.toml

/run cargo check --message-format=short
/run cargo build
```

**vibe**
This is a web server written in rust using axum. It is for uploading and downloading files.

In the `download_file_handler` function, compress the file using `zstd` and encrypt using `cryptostream`, and send the encypted stream to the http client. The client will take care of decrypting and decompressing on its side.
