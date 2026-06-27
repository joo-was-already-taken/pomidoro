#!/usr/bin/env bats

load "${BATS_TEST_DIRNAME}/test-helper.bash"

setup() {
	common_setup
}

teardown() {
	common_teardown
}

@test "next interval command advances to next phase" {
	start_server

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" next
	(( status == 0 ))
	[[ "$output" == "" ]]
}

@test "correct next interval" {
	start_server

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" next >/dev/null
	sleep 0.1

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
	(( status == 0 ))
	[[ "$output" == *"break"* ]]
}

@test "timer countdown: time left decreases" {
	start_server

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" start >/dev/null
	sleep 0.1

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status --json
	TIME_LEFT_1=$(echo "$output" | jq '.time_left')

	sleep 1

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status --json
	TIME_LEFT_2=$(echo "$output" | jq '.time_left')

	(( TIME_LEFT_2 <= TIME_LEFT_1 ))
	}

	@test "timer paused: time_left stays constant" {
	start_server

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" start >/dev/null
	sleep 0.1
	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" pause >/dev/null

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status --json
	TIME_LEFT_1=$(echo "$output" | jq '.time_left')

	sleep 1

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status --json
	TIME_LEFT_2=$(echo "$output" | jq '.time_left')

	(( TIME_LEFT_2 == TIME_LEFT_1 ))
	}

@test "cycle wraps: focus -> break -> focus" {
	start_server

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
	[[ "$output" == *"focus"* ]]

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" next >/dev/null
	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
	[[ "$output" == *"break"* ]]

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" next >/dev/null
	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
	[[ "$output" == *"focus"* ]]
}

@test "cycle with three interval types" {
	THREE_CONFIG="${TEST_DIR}/three.toml"
	cat > "${THREE_CONFIG}" <<-'EOF'
		cycle = ["phase1", "phase2", "phase3"]

		[socket]
		addr = "pomidoro-three-${uid}-$$"
		abstract = true

		[intervals.phase1]
		duration = "2s"
		productive = true

		[intervals.phase2]
		duration = "1s"
		productive = false

		[intervals.phase3]
		duration = "2s"
		productive = false
	EOF

	"${POMIDORO_BIN}" --config "${THREE_CONFIG}" start-server &
	SERVER_PID=$!
	export SERVER_PID

	local retry=0
	while (( retry < 50 )); do
		if "${POMIDORO_BIN}" --config "${THREE_CONFIG}" status &>/dev/null; then
			break
		fi
		sleep 0.05
		retry=$((retry + 1))
	done

	run "${POMIDORO_BIN}" --config "${THREE_CONFIG}" status
	[[ "$output" == *"phase1"* ]]

	"${POMIDORO_BIN}" --config "${THREE_CONFIG}" next >/dev/null
	run "${POMIDORO_BIN}" --config "${THREE_CONFIG}" status
	[[ "$output" == *"phase2"* ]]

	"${POMIDORO_BIN}" --config "${THREE_CONFIG}" next >/dev/null
	run "${POMIDORO_BIN}" --config "${THREE_CONFIG}" status
	[[ "$output" == *"phase3"* ]]
}

@test "next interval from paused state starts running next interval" {
	start_server

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" start >/dev/null
	sleep 0.2
	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" pause >/dev/null

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
	[[ "$output" == *"paused"* ]]

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" next >/dev/null
	sleep 0.1

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
	[[ "$output" == *"break"* ]]
	[[ "$output" == *"running"* ]]
}

@test "next interval from stopped state starts running next interval" {
	start_server

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
	[[ "$output" == *"stopped"* ]]

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" next >/dev/null
	sleep 0.1

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
	[[ "$output" == *"break"* ]]
	[[ "$output" == *"running"* ]]
}

@test "next interval from overtime state starts running next interval" {
	OVERTIME_CONFIG="${TEST_DIR}/overtime.toml"
	cat > "${OVERTIME_CONFIG}" <<-'EOF'
		cycle = ["focus", "break"]

		[socket]
		addr = "pomidoro-overtime-${uid}-$$"
		abstract = true

		[intervals.focus]
		duration = "1s"
		productive = true

		[intervals.break]
		duration = "1s"
		productive = false
	EOF

	"${POMIDORO_BIN}" --config "${OVERTIME_CONFIG}" start-server &
	SERVER_PID=$!
	export SERVER_PID

	local retry=0
	while (( retry < 50 )); do
		if "${POMIDORO_BIN}" --config "${OVERTIME_CONFIG}" status &>/dev/null; then
			break
		fi
		sleep 0.05
		retry=$((retry + 1))
	done

	"${POMIDORO_BIN}" --config "${OVERTIME_CONFIG}" start >/dev/null
	sleep 1.2

	run "${POMIDORO_BIN}" --config "${OVERTIME_CONFIG}" status --json
	[[ "$output" == *"\"is_overtime\":true"* ]]

	"${POMIDORO_BIN}" --config "${OVERTIME_CONFIG}" next >/dev/null
	sleep 0.1

	run "${POMIDORO_BIN}" --config "${OVERTIME_CONFIG}" status --json
	[[ "$output" == *"break"* ]]
	[[ "$output" == *"\"state\":\"Running\""* ]]
	[[ "$output" == *"\"is_overtime\":false"* ]]
}
