====

# TODO

set filetime
refresh_file_db
get_file_db

====

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
curl --fail-with-body http://127.0.0.1:34890/

curl -X POST http://127.0.0.1:34890/upload \
  -H "X-File-Name: sample.pdf" \
  -H "X-File-Time: 2026-03-30T10:00:00Z" \
  -H "X-Sub-Dir: /home/user/documents/sample.txt" \
  --data-binary "@/home/ray/MEGA/Rays/prescription - glasses - 2014-08-10.pdf"

curl -X POST http://127.0.0.1:34890/upload \
  -H "X-File-Name: notexist.not" \
  -H "X-File-Time: 2026-03-30T10:00:00Z" \
  --data-binary "@/home/ray/MEGA/Rays/notexist.not"

curl --fail-with-body -H "file-path: Programming/ideas.txt" \
  http://127.0.0.1:34890/download \
  -o ideas.txt

curl -H "file-path: subdir/sample.pdf" \
  http://127.0.0.1:34890/download \
  -o sample.pdf

curl --fail-with-body -H "file-path: notexist.not" \
  http://127.0.0.1:34890/download \
  -o notexist.not

curl --fail-with-body http://127.0.0.1:34890/refresh_file_db

curl --fail-with-body http://127.0.0.1:34890/get_file_db -o metadata.db
```

========

# Cloudflare

https://fs.rayzinnz.com
-> http://localhost:34890

cloudflared tunnel create ray-tunnel

--get a token and install service (cloudflare\domain\networking\tunnels)
https://dash.cloudflare.com/95f5157928298c0b03d5435f585cab35/tunnels/7b661e9d-a9e2-4885-869c-b6a8b3e45ac8/overview
sudo cloudflared service install <<base64string>>

--migrate tunnel from locally managed to dashboard managed
https://dash.cloudflare.com/95f5157928298c0b03d5435f585cab35/one/networks/connectors/cloudflare-tunnels/7b661e9d-a9e2-4885-869c-b6a8b3e45ac8/migrate

