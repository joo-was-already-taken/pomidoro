#!/usr/bin/env bats

load "${BATS_TEST_DIRNAME}/test-helper.bash"

setup() {
	common_setup
}

teardown() {
	common_teardown
}

@test "status prints tab separated values by default" {
	start_server

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
	(( status == 0 ))
	[[ "$output" == *"focus"$'\t'"stopped"$'\t'"false"* ]]
}

@test "status respects --data argument" {
	start_server

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status --data state,interval
	(( status == 0 ))
	[[ "$output" == "stopped"$'\t'"focus" ]]
}

@test "status --json outputs JSON" {
	start_server

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status --json
	(( status == 0 ))

	echo "$output" | jq -e '.state == "Stopped"' >/dev/null
}

@test "simple commands are silent on success" {
	start_server

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" start
	(( status == 0 ))
	[[ "$output" == "" ]]

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" pause
	(( status == 0 ))
	[[ "$output" == "" ]]
}
