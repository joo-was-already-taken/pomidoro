#!/usr/bin/env bats

load 'test-helper'

setup() {
	common_setup
}

teardown() {
	common_teardown
}

@test "overtime hook executes periodically" {
	local hook_file="${TEST_DIR}/hook_overtime.txt"

	cat > "${CONFIG_FILE}" <<-EOF
		cycle = ["work"]

		[socket]
		addr = "$(get_socket_name)"
		abstract = true

		[intervals.work]
		duration = "1s"

		[hooks.overtime]
		every = "1s"
		execute = "echo 'overtime' >> ${hook_file}"
	EOF

	start_server
	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" start

	sleep 3.5

	[[ -f "${hook_file}" ]]

	local lines
	lines=$(wc -l < "${hook_file}")

	(( lines >= 2 ))
}

@test "on_completion hook executes exactly once when entering overtime" {
	local hook_file="${TEST_DIR}/hook_completion.txt"

	cat > "${CONFIG_FILE}" <<-EOF
		cycle = ["work"]

		[socket]
		addr = "$(get_socket_name)"
		abstract = true

		[intervals.work]
		duration = "1s"

		[hooks]
		on_completion = "echo 'complete' >> ${hook_file}"
	EOF

	start_server
	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" start

	sleep 1.2

	[[ -f "${hook_file}" ]]
	local lines
	lines=$(wc -l < "${hook_file}")
	(( lines == 1 ))
}

@test "on_pause and on_resume hooks execute on toggle" {
	local pause_file="${TEST_DIR}/pause.txt"
	local resume_file="${TEST_DIR}/resume.txt"

	cat > "${CONFIG_FILE}" <<-EOF
		cycle = ["work"]

		[socket]
		addr = "$(get_socket_name)"
		abstract = true

		[intervals.work]
		duration = "10s"

		[hooks]
		on_pause = "echo 'paused' >> ${pause_file}"
		on_resume = "echo 'resumed' >> ${resume_file}"
	EOF

	start_server
	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" start
	sleep 0.1

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" toggle
	sleep 0.1
	[[ -f "${pause_file}" ]]

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" toggle
	sleep 0.1
	[[ -f "${resume_file}" ]]
}

@test "interval-specific hook takes precedence over global hook" {
	local work_file="${TEST_DIR}/work.txt"
	local global_file="${TEST_DIR}/global.txt"

	cat > "${CONFIG_FILE}" <<-EOF
		cycle = ["work"]

		[socket]
		addr = "$(get_socket_name)"
		abstract = true

		[intervals.work]
		duration = "10s"

		[intervals.work.hooks]
		on_pause = "echo 'work paused' >> ${work_file}"

		[hooks]
		on_pause = "echo 'global paused' >> ${global_file}"
	EOF

	start_server
	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" start
	sleep 0.1

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" toggle
	sleep 0.1

	[[ -f "${work_file}" ]]
	[[ ! -f "${global_file}" ]]
}

@test "long running hooks do not block the server" {
	local hook_file="${TEST_DIR}/long_hook.txt"

	cat > "${CONFIG_FILE}" <<-EOF
		cycle = ["work"]

		[socket]
		addr = "$(get_socket_name)"
		abstract = true

		[intervals.work]
		duration = "10s"

		[hooks]
		on_pause = "sleep 0.5 && echo 'finished' >> ${hook_file}"
	EOF

	start_server
	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" start
	sleep 0.1

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" toggle

	local t1
	t1=$(date +%s%N)
	run "${POMIDORO_BIN}" --config "${CONFIG_FILE}" status
	local t2
	t2=$(date +%s%N)

	(( status == 0 ))
	[[ "$output" == *"paused"* ]]

	local duration=$(( (t2 - t1) / 1000000 ))
	(( duration < 300 ))

	[ ! -f "${hook_file}" ]
	sleep 0.8
	[ -f "${hook_file}" ]
}

@test "subsequent fast hook finishes before earlier slow hook" {
	local hook_file="${TEST_DIR}/concurrency.txt"

	cat > "${CONFIG_FILE}" <<-EOF
		cycle = ["work"]

		[socket]
		addr = "$(get_socket_name)"
		abstract = true

		[intervals.work]
		duration = "10s"

		[hooks]
		on_pause = "sleep 0.5 && echo 'slow' >> ${hook_file}"
		on_resume = "echo 'fast' >> ${hook_file}"
	EOF

	start_server
	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" start
	sleep 0.1

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" toggle
	sleep 0.1

	"${POMIDORO_BIN}" --config "${CONFIG_FILE}" toggle

	sleep 0.8

	[ -f "${hook_file}" ]

	local line1
	local line2
	line1=$(sed -n '1p' "${hook_file}")
	line2=$(sed -n '2p' "${hook_file}")

	[ "$line1" = "fast" ]
	[ "$line2" = "slow" ]
}
