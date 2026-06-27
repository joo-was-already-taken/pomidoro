#!/usr/bin/env bats

load "${BATS_TEST_DIRNAME}/test-helper.bash"

setup() {
	common_setup
}

teardown() {
	common_teardown
}

@test "config command outputs correct JSON configuration" {
	start_server

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" config
	(( status == 0 ))

	[[ "$output" == *"\"cycle\":[\"focus\",\"break\"]"* ]]
	[[ "$output" == *"\"intervals\":{"* ]]
	[[ "$output" == *"\"focus\":{\"productive\":true,\"duration\":5}"* ]]
	[[ "$output" == *"\"break\":{\"productive\":false,\"duration\":2}"* ]]
}

@test "config command fails gracefully when server is not running" {
	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" config

	(( status != 0 ))
	[[ "$output" == *"Connection refused"* ]] \
		|| [[ "$output" == *"No such file or directory"* ]]
}
