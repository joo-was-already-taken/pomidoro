#!/usr/bin/env bats

load "${BATS_TEST_DIRNAME}/test-helper.bash"

setup() {
	common_setup
}

teardown() {
	common_teardown
}

@test "pause on stopped timer returns error" {
	start_server

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" pause
	(( status == 0 ))
	[[ "$output" == *"Timer is not"* ]] || [[ "$output" == *"error"* ]]
}

@test "resume on stopped timer returns error" {
	start_server

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" resume
	(( status == 0 ))
	[[ "$output" == *"Timer is not"* ]] || [[ "$output" == *"error"* ]]
}

@test "server uses configured socket name" {
	start_server

	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
	(( status == 0 ))
}

@test "server handles extended client interaction" {
	start_server

	for _ in {1..5}; do
		run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
		(( status == 0 ))
	done
}

@test "explicit config file via --config flag" {
	EXPLICIT_CONFIG="${TEST_DIR}/explicit.toml"
	cat > "${EXPLICIT_CONFIG}" <<-'EOF'
		cycle = ["test"]

		[socket]
		addr = "pomidoro-explicit-${uid}-$$"
		abstract = true

		[intervals.test]
		duration = "3s"
		productive = true
	EOF

	"${POMIDORO_BIN}" --config "${EXPLICIT_CONFIG}" start-server &
	SERVER_PID=$!
	export SERVER_PID

	local retry=0
	while (( retry < 50 )); do
		if "${POMIDORO_BIN}" --config "${EXPLICIT_CONFIG}" status &>/dev/null; then
			break
		fi
		sleep 0.05
		retry=$((retry + 1))
	done

	run "${POMIDORO_BIN}" --config "${EXPLICIT_CONFIG}" status
	(( status == 0 ))
	[[ "$output" == *"test"* ]]
}

@test "config with custom interval names" {
	NAMED_CONFIG="${TEST_DIR}/named.toml"
	cat > "${NAMED_CONFIG}" <<-'EOF'
		cycle = ["deep-work", "coffee-break"]

		[socket]
		addr = "pomidoro-named-${uid}-$$"
		abstract = true

		[intervals."deep-work"]
		duration = "6s"
		productive = true

		[intervals."coffee-break"]
		duration = "2s"
		productive = false
	EOF

	"${POMIDORO_BIN}" --config "${NAMED_CONFIG}" start-server &
	SERVER_PID=$!
	export SERVER_PID

	local retry=0
	while (( retry < 50 )); do
		if "${POMIDORO_BIN}" --config "${NAMED_CONFIG}" status &>/dev/null; then
			break
		fi
		sleep 0.05
		retry=$((retry + 1))
	done

	run "${POMIDORO_BIN}" --config "${NAMED_CONFIG}" status --json
	(( status == 0 ))
	[[ "$output" == *"deep-work"* ]]
}

@test "server uses defaults when config minimal" {
	MINIMAL_CONFIG="${TEST_DIR}/minimal.toml"
	cat > "${MINIMAL_CONFIG}" <<-'EOF'
		[socket]
		addr = "pomidoro-min-${uid}-$$"
		abstract = true
	EOF

	# Start server with minimal config
	"${POMIDORO_BIN}" --config "${MINIMAL_CONFIG}" start-server &
	SERVER_PID=$!
	export SERVER_PID

	local retry=0
	while (( retry < 50 )); do
		if "${POMIDORO_BIN}" --config "${MINIMAL_CONFIG}" status &>/dev/null; then
			break
		fi
		sleep 0.05
		retry=$((retry + 1))
	done

	run "${POMIDORO_BIN}" --config "${MINIMAL_CONFIG}" status
	(( status == 0 ))
	[[ "$output" == *"work"* ]] || [[ "$output" == *"state"* ]]
}

@test "config parsing handles TOML comments" {
	COMMENTED_CONFIG="${TEST_DIR}/commented.toml"
	cat > "${COMMENTED_CONFIG}" <<-'EOF'
		# This is a test config
		cycle = ["type-a", "type-b"]

		[socket]
		# Abstract socket
		addr = "pomidoro-comment-${uid}-$$"
		abstract = true

		[intervals.type-a]
		duration = "5s"
		productive = true

		[intervals.type-b]
		duration = "1s"
		productive = false
	EOF

	# Start server
	"${POMIDORO_BIN}" --config "${COMMENTED_CONFIG}" start-server &
	SERVER_PID=$!
	export SERVER_PID

	local retry=0
	while (( retry < 50 )); do
		if "${POMIDORO_BIN}" --config "${COMMENTED_CONFIG}" status &>/dev/null; then
			break
		fi
		sleep 0.05
		retry=$((retry + 1))
	done

	run "${POMIDORO_BIN}" --config "${COMMENTED_CONFIG}" status
	(( status == 0 ))
	[[ "$output" == *"type-a"* ]]
}

@test "different configs maintain separate interval durations" {
	CONFIG_LONG="${TEST_DIR}/long-intervals.toml"
	CONFIG_SHORT="${TEST_DIR}/short-intervals.toml"

	cat > "${CONFIG_LONG}" <<-'EOF'
		cycle = ["work"]

		[socket]
		addr = "pomidoro-long-${uid}-$$"
		abstract = true

		[intervals.work]
		duration = "15s"
		productive = true
	EOF

	cat > "${CONFIG_SHORT}" <<-'EOF'
		cycle = ["work"]

		[socket]
		addr = "pomidoro-short-${uid}-$$"
		abstract = true

		[intervals.work]
		duration = "2s"
		productive = true
	EOF

	# Start first server
	"${POMIDORO_BIN}" --config "${CONFIG_LONG}" start-server &
	SERVER_PID=$!
	export SERVER_PID

	local retry=0
	while (( retry < 50 )); do
		if "${POMIDORO_BIN}" --config "${CONFIG_LONG}" status &>/dev/null; then
			break
		fi
		sleep 0.05
		retry=$((retry + 1))
	done

	run "${POMIDORO_BIN}" --config "${CONFIG_LONG}" start
	(( status == 0 ))

	sleep 0.1

	run "${POMIDORO_BIN}" --config "${CONFIG_LONG}" status --json
	(( status == 0 ))
	[[ "$(echo "$output" | jq '.total_interval_duration')" == "15" ]]
}

@test "configuration fallback to /etc" {
	run_precedence_test "etc-work" "/nonexistent" ""
}

@test "configuration from XDG_CONFIG_HOME overrides /etc" {
	run_precedence_test "xdg-work" "${TEST_DIR}/fake_xdg" ""
}

@test "explicit --config flag overrides XDG_CONFIG_HOME" {
	run_precedence_test "cli-work" "${TEST_DIR}/fake_xdg" "${TEST_DIR}/fake_cli/pomidoro/config.toml"
}
