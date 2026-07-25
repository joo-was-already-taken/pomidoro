#!/usr/bin/env bats

load "${BATS_TEST_DIRNAME}/test-helper.bash"

setup() {
	common_setup
}

teardown() {
	common_teardown
}

@test "start timer command succeeds" {
	start_server

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" start
	(( status == 0 ))
	[[ "$output" == "" ]]
}

@test "status shows running state after start" {
	start_server

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" start >/dev/null
	sleep 0.2

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
	(( status == 0 ))
	[[ "$output" == *"running"* ]]
}

@test "pause stops running timer" {
	start_server

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" start >/dev/null
	sleep 0.2

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" pause
	(( status == 0 ))
	[[ "$output" == "" ]]
}

@test "status shows paused state after pause" {
	start_server

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" start >/dev/null
	sleep 0.2
	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" pause >/dev/null
	sleep 0.1

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
	(( status == 0 ))
	[[ "$output" == *"paused"* ]]
}

@test "resume restarts paused timer" {
	start_server

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" start >/dev/null
	sleep 0.2
	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" pause >/dev/null
	sleep 0.1

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" resume
	(( status == 0 ))
	[[ "$output" == "" ]]
}

@test "toggle alternates between pause and resume" {
	start_server

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" start >/dev/null
	sleep 0.2

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" toggle >/dev/null
	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
	[[ "$output" == *"paused"* ]]

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" toggle >/dev/null
	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
	[[ "$output" == *"running"* ]]
}

@test "stop command resets timer" {
	start_server

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" start >/dev/null
	sleep 0.2

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" stop
	(( status == 0 ))
	[[ "$output" == "" ]]
}

@test "status shows stopped state after stop" {
	start_server

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" start >/dev/null
	sleep 0.2
	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" stop >/dev/null
	sleep 0.1

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
	(( status == 0 ))
	[[ "$output" == *"stopped"* ]]
}

@test "server initializes with stopped timer" {
	start_server

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
	(( status == 0 ))
	[[ "$output" == *"stopped"* ]]
}

setup_path_socket_config() {
	export SOCKET_FILE="${TEST_DIR}/sockets/nested/pomidoro.sock"
	cat > "${CONFIG_FILE}" <<-EOF
		cycle = ["focus", "break"]

		[socket]
		addr = "${SOCKET_FILE}"
		abstract = false

		[intervals.focus]
		duration = "5s"
		productive = true

		[intervals.break]
		duration = "2s"
		productive = false
	EOF
}

@test "path socket: creates parent directories and binds" {
	setup_path_socket_config
	[[ ! -d "${TEST_DIR}/sockets" ]]

	start_server

	[[ -S "${SOCKET_FILE}" ]]
}

@test "path socket: removes stale socket and starts successfully" {
	setup_path_socket_config
	mkdir -p "$(dirname "${SOCKET_FILE}")"
	touch "${SOCKET_FILE}"

	[[ -f "${SOCKET_FILE}" ]]
	[[ ! -S "${SOCKET_FILE}" ]]

	start_server

	[[ -S "${SOCKET_FILE}" ]]
}

@test "path socket: fails to start if server is already running" {
	setup_path_socket_config
	start_server

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" start-server
	(( status == 1 ))
	[[ "$output" == *"Server is already running on this socket"* ]]
}
