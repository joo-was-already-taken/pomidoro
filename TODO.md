# Pomidoro TODO list

- [ ] create sockets in users' namespaces (e.g. `/tmp/pomidoro/<user>/server0.sock`)
- [ ] more user-friendly error handling (currently returning them from `main`)
- [x] refactor getting socket directory across server and client code
- [x] create `Config` struct, in which every value is initialized as opposed to `TomlConfig`
- [ ] handle invalid time formats
- [x] remove server socket if already exists
- [ ] add timeout for server response
