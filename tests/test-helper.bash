set -euo pipefail

# Returns a unique socket name for the current test
get_socket_name() {
	local suffix="${1:-main}"
	echo "pomidoro-${BATS_TEST_FILENAME##*/}-${BATS_TEST_NUMBER}-${suffix}-$$"
}

common_setup() {
	local test_dir
	test_dir="$(mktemp -d)"
	export TEST_DIR="$test_dir"

	if [[ -z "${POMIDORO_BIN:-}" ]]; then
		local test_file_dir
		test_file_dir="$(cd "${BATS_TEST_DIRNAME}/.." && pwd)"

		if [[ ! -f "$test_file_dir/target/debug/pomidoro" ]]; then
			(cd "$test_file_dir" && cargo build --quiet)
		fi
		export POMIDORO_BIN="$test_file_dir/target/debug/pomidoro"
	fi

	export CONFIG_FILE="${TEST_DIR}/config.toml"
	export RUST_LOG=error
	export SHELL=sh

	cat > "${CONFIG_FILE}" <<EOF
		cycle = ["focus", "break"]

		[socket]
		addr = "$(get_socket_name)"
		abstract = true

		[intervals.focus]
		duration = "5s"
		productive = true

		[intervals.break]
		duration = "2s"
		productive = false
EOF
}

common_teardown() {
	if [[ -n "${SERVER_PID:-}" ]]; then
		kill "$SERVER_PID" 2>/dev/null || true
		wait "$SERVER_PID" 2>/dev/null || true
	fi

	if [[ -n "${TEST_DIR:-}" ]] && [[ -d "${TEST_DIR}" ]]; then
		rm -rf "${TEST_DIR}"
	fi
}

# Helper to run commands in bwrap isolation
run_in_bwrap() {
	local bwrap_tmpdir="${TEST_DIR}/bwrap-$$-$RANDOM"
	mkdir -p "${bwrap_tmpdir}"

	local bwrap_cmd=(
		bwrap
		--die-with-parent
		--unshare-pid
		--unshare-ipc
		--unshare-uts
		--unshare-net
		--bind "${TEST_DIR}" "${TEST_DIR}"
		--bind /usr /usr
		--bind /lib /lib
		--bind /lib64 /lib64
		--bind /bin /bin
		--tmpfs /tmp
		--tmpfs /root
		--dev /dev
		--proc /proc
		--chdir "${bwrap_tmpdir}"
	)

	if [[ -d /sbin ]]; then
		bwrap_cmd+=(--bind /sbin /sbin)
	fi

	"${bwrap_cmd[@]}" "$@"
}

run_pomidoro_in_bwrap() {
	run_in_bwrap \
		"${POMIDORO_BIN}" \
		--config "${CONFIG_FILE}" \
		"$@"
}

# Helper to start server in bwrap and wait for readiness
# Returns the PID of the server process (outside bwrap)
start_server() {
	"${POMIDORO_BIN}" \
		--config "${CONFIG_FILE}" \
		start-server &
	SERVER_PID=$!
	export SERVER_PID

	# Wait for server to be ready (poll status command)
	local max_retries=100
	local retry=0
	while (( retry < max_retries )); do
		if "${POMIDORO_BIN}" \
			--config "${CONFIG_FILE}" \
			status &>/dev/null; then
			return 0
		fi
		sleep 0.05
		retry=$((retry + 1))
	done

	kill "$SERVER_PID" 2>/dev/null || true
	return 1
}

start_server_in_bwrap() {
	run_in_bwrap \
		"${POMIDORO_BIN}" \
		--config "${CONFIG_FILE}" \
		start-server &
	SERVER_PID=$!
	export SERVER_PID

	# Wait for server to be ready (poll status command)
	local max_retries=100
	local retry=0
	while (( retry < max_retries )); do
		if run_pomidoro_in_bwrap status &>/dev/null 2>&1; then
			return 0
		fi
		sleep 0.05
		retry=$((retry + 1))
	done

	kill "$SERVER_PID" 2>/dev/null || true
	return 1
}

# Helper for config precedence tests
setup_fake_configs() {
	export FAKE_ETC="${TEST_DIR}/fake_etc"
	export FAKE_XDG="${TEST_DIR}/fake_xdg"
	export FAKE_CLI="${TEST_DIR}/fake_cli"
	mkdir -p "${FAKE_ETC}/pomidoro" "${FAKE_XDG}/pomidoro" "${FAKE_CLI}/pomidoro"

	cat > "${FAKE_ETC}/pomidoro/config.toml" <<-EOF
		cycle = ["etc-work"]
		[intervals.etc-work]
		duration = "1s"
		productive = true
		[socket]
		addr = "$(get_socket_name etc)"
		abstract = true
	EOF

	cat > "${FAKE_XDG}/pomidoro/config.toml" <<-EOF
		cycle = ["xdg-work"]
		[intervals.xdg-work]
		duration = "1s"
		productive = true
		[socket]
		addr = "$(get_socket_name xdg)"
		abstract = true
	EOF

	cat > "${FAKE_CLI}/pomidoro/config.toml" <<-EOF
		cycle = ["cli-work"]
		[intervals.cli-work]
		duration = "1s"
		productive = true
		[socket]
		addr = "$(get_socket_name cli)"
		abstract = true
	EOF
}

run_precedence_test() {
	local expected_pattern="$1"
	local xdg_env="$2"
	local extra_config_flag="$3"

	setup_fake_configs

	local -a cmd=("${POMIDORO_BIN}")
	if [[ -n "${extra_config_flag}" ]]; then
		cmd+=("--config" "${extra_config_flag}")
	fi

	bwrap \
		--bind / / \
		--bind "${FAKE_ETC}" /etc \
		--setenv XDG_CONFIG_HOME "${xdg_env}" \
		--setenv HOME "/nonexistent" \
		--setenv RUST_LOG error \
		--unshare-ipc \
		--unshare-pid \
		--die-with-parent \
		"${cmd[@]}" start-server >/dev/null 2>&1 < /dev/null &
	SERVER_PID=$!

	local retry=0
	local success=0
	while (( retry < 50 )); do
		if bwrap \
			--bind / / \
			--bind "${FAKE_ETC}" /etc \
			--setenv XDG_CONFIG_HOME "${xdg_env}" \
			--setenv HOME "/nonexistent" \
			--unshare-ipc "${cmd[@]}" status 2>/dev/null | grep -q "${expected_pattern}"; then
			success=1
			break
		fi
		sleep 0.05
		retry=$((retry + 1))
	done

	kill -9 $SERVER_PID 2>/dev/null || true
	wait $SERVER_PID 2>/dev/null || true
	(( success == 1 ))
}
