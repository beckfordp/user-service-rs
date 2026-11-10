# Spec — RUST-3.4: Logs

## Overview
gluon PLAN.md RUST-3.4 (ADR 0028 "Logs" row). purerest `Logging`
(trace-correlated structured logger named after the service) writing what
logstash-logback-encoder's `LogstashEncoder` writes in the Docker images
(`logback-docker.xml`), and user-service's log calls. Harness:
`tests/contract` `checks/test_logs.py` and the config group's logger-name
check (RUST-1.7, RUST-1.9). Sources: capture 20261109 `stdout.log`; purerest
`Logging`, `logback-docker.xml`; user-service `Main`, `AuthRoutes`,
`UserRoutes`; filebeat.yml (only lines starting `{"@timestamp"` decode).

## Functional Requirements
1. One JSON object per stdout line: `@timestamp` (UTC, ISO-8601, fraction
   trimmed of trailing zeros, Java clock resolution), `@version` `"1"`,
   `message`, `logger_name` (= `USER_SERVICE_NAME`), `thread_name`, `level`,
   `level_value` (INFO 20000, WARN 30000, ERROR 40000), then the call's context
   fields, then `trace_id`/`span_id` when a span is current.
2. Startup: INFO `user-service starting` with `port` and `metrics_port` (strings).
3. Requests, with the context maps of `AuthRoutes`/`UserRoutes`: `Received
   request`; `Invalid signup request`/`Invalid login request` (WARN, `error` =
   the response text); `Duplicate signup` (WARN); `Account created` (INFO,
   `user_id`); `Login failed` (WARN); `Stored password hash is malformed`
   (ERROR, `user_id`); `Login succeeded` (INFO, `user_id`); users: `path`
   `/users/<canonical id>` and `user_id`, `Request completed` (INFO) or `User not
   found` (WARN). Undecodable requests log nothing. No password or token is
   ever logged.
4. Unexpected failures inside a request log an ERROR line (`Request failed`,
   `error`) instead of plain stderr text.

## Acceptance Criteria
- `scripts/verify-logs.sh` PASS: CI green; contract logs group green; config
  group's logger-name check green.
