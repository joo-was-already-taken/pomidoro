#!/usr/bin/env bats

load "${BATS_TEST_DIRNAME}/test-helper.bash"

setup() {
	common_setup
}

teardown() {
	common_teardown
}

@test "listen --events command streams correct JSON events" {
	start_server

	local events_file="${TEST_DIR}/events.log"

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" listen --events > "${events_file}" &
	local LISTEN_PID=$!

	sleep 0.1

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" start
	sleep 0.1

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" pause
	sleep 0.1

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" resume
	sleep 0.1

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" stop
	sleep 0.1

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" next
	sleep 0.1

	kill -9 "${LISTEN_PID}" 2>/dev/null || true
	wait "${LISTEN_PID}" 2>/dev/null || true

	[[ -f "${events_file}" ]]
	local log_content
	log_content=$(cat "${events_file}")

	[[ "$log_content" == *"\"type\":\"Start\""* ]]
	[[ "$log_content" == *"\"interval\":{\"name\":\"focus\",\"productive\":true,\"duration\":5}"* ]]
	[[ "$log_content" == *"\"type\":\"Pause\""* ]]
	[[ "$log_content" == *"\"type\":\"Resume\""* ]]
	[[ "$log_content" == *"\"type\":\"Stop\""* ]]
	[[ "$log_content" == *"\"type\":\"Next\""* ]]
	[[ "$log_content" == *"\"finished_interval\":{\"name\":\"focus\",\"productive\":true,\"duration\":5}"* ]]
	[[ "$log_content" == *"\"started_interval\":{\"name\":\"break\",\"productive\":false,\"duration\":2}"* ]]
}

@test "listen --events emmits interval completed event when timer naturally finishes" {
	local edge_config="${TEST_DIR}/edge.toml"
	cat > "${edge_config}" <<-EOF
		cycle = ["short"]
		[socket]
		addr = "$(get_socket_name edge)"
		abstract = true
		[intervals.short]
		duration = "1s"
		productive = true
	EOF

	"${POMIDORO_BIN}" --config "${edge_config}" start-server &
	SERVER_PID=$!
	export SERVER_PID

	local retry=0
	while (( retry < 50 )); do
		if "${POMIDORO_BIN}" --config "${edge_config}" status &>/dev/null; then
			break
		fi
		sleep 0.05
		retry=$((retry + 1))
	done

	local events_file="${TEST_DIR}/events_edge.log"
	"${POMIDORO_BIN}" --config "${edge_config}" listen --events > "${events_file}" &
	local LISTEN_PID=$!
	sleep 0.2

	"${POMIDORO_BIN}" --config "${edge_config}" start

	sleep 1.5

	kill -9 "${LISTEN_PID}" 2>/dev/null || true
	wait "${LISTEN_PID}" 2>/dev/null || true

	[[ -f "${events_file}" ]]
	local log_content
	log_content=$(cat "${events_file}")

	[[ "$log_content" == *"\"type\":\"Start\""* ]]
	[[ "$log_content" == *"\"type\":\"IntervalCompleted\""* ]]
}

@test "listen --events ignores redundant commands (no event spam)" {
	start_server

	local events_file="${TEST_DIR}/events_redundant.log"

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" listen --events > "${events_file}" &
	local LISTEN_PID=$!
	sleep 0.1

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" start
	sleep 0.1
	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" start
	sleep 0.1
	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" start
	sleep 0.2

	kill -9 "${LISTEN_PID}" 2>/dev/null || true
	wait "${LISTEN_PID}" 2>/dev/null || true

	local log_content
	log_content=$(cat "${events_file}")

	local count
	count=$(echo "$log_content" | grep -c "\"type\":\"Start\"")
	(( count == 1 ))
}
