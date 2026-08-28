# Design

client (work)
  - update cache
  - request file db
  - run diff

server
  - on recieve of file db request, send file db

so server needs to serve
  1. Path to file db
  2. Root path of synced folder

===

http://127.0.0.1:34890

curl -X POST --json '{"id": "12", "username": "abc"}' http://127.0.0.1:34890/users

```sh
curl -X POST http://127.0.0.1:34890/upload \
  -H "X-File-Name: sample.pdf" \
  -H "X-File-Time: 2026-03-30T10:00:00Z" \
  -H "X-Sub-Dir: /home/user/documents/sample.txt" \
  --data-binary "@/home/ray/MEGA/Rays/prescription - glasses - 2014-08-10.pdf"

curl -H "file-path: subdir/sample.txt" \
  http://127.0.0.1:34890/download \
  -o downloaded-file.txt

curl -H "file-path: subdir/sample.pdf" \
  http://127.0.0.1:34890/download \
  -o sample.pdf
```
