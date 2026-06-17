#!/usr/bin/env bats

load "${BATS_TEST_DIRNAME}/test-helper.bash"

setup() {
	common_setup
}

teardown() {
	common_teardown
}

@test "client and server communicate via abstract socket" {
	start_server

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
	(( status == 0 ))
	[[ "$output" == *"Stopped"* ]] || [[ "$output" == *"state"* ]]
}

@test "multiple clients can command same server sequentially" {
	start_server

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" start
	(( status == 0 ))

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
	(( status == 0 ))
	[[ "$output" == *"Running"* ]]

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" pause
	(( status == 0 ))

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
	(( status == 0 ))
	[[ "$output" == *"Paused"* ]]
}

@test "command aliases" {
	start_server

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" s
	(( status == 0 ))

	sleep 0.2

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" p
	(( status == 0 ))
	[[ "$output" == *"success"* ]]
}

@test "responses contain JSON-like structure" {
	start_server

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
	(( status == 0 ))

	echo "$output" | jq . >/dev/null
}

@test "next command alias 'n' works" {
	start_server

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" n
	(( status == 0 ))
	[[ "$output" == *"success"* ]]
}

@test "server responds to status and all command types" {
	start_server

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
	(( status == 0 ))

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" start
	(( status == 0 ))

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" next
	(( status == 0 ))

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" pause
	(( status == 0 ))
}

@test "confirmation responses include required JSON fields" {
	start_server

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" start
	(( status == 0 ))

	[[ "$(echo "$output" | jq -r '.request')" == "Start" ]]
	[[ "$(echo "$output" | jq -r '.success')" == "true" ]]
	echo "$output" | jq -e 'has("error_msg")' >/dev/null
}

@test "status responses include complete timer information" {
	start_server

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" start >/dev/null

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
	(( status == 0 ))

	echo "$output" | jq -e 'has("interval_type")' >/dev/null
	echo "$output" | jq -e 'has("state")' >/dev/null
	echo "$output" | jq -e 'has("is_overtime")' >/dev/null
	echo "$output" | jq -e 'has("overtime")' >/dev/null
	echo "$output" | jq -e 'has("time_left")' >/dev/null
	echo "$output" | jq -e 'has("time_elapsed")' >/dev/null
	echo "$output" | jq -e 'has("total_interval_duration")' >/dev/null
}
