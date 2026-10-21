;; A test rule hook for tools/compat/hooks.sh: once a game minute it draws a number below 1000
;; from the game's random stream (which changes everything after it) and posts a line with the
;; number's last digit.
(module
  (import "unbanished" "time_in_year" (func $time (result i64)))
  (import "unbanished" "random_below" (func $random (param i32) (result i32)))
  (import "unbanished" "post_event" (func $post (param i32 i32 i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "A minute: 0")
  (func (export "api_version") (result i32) i32.const 1)
  (func (export "after_iteration")
    ;; Once a minute: the first iteration after each 60 s of game time.
    (if (i64.lt_u (i64.rem_u (call $time) (i64.const 60000000)) (i64.const 16666))
      (then
        (i32.store8 (i32.const 10)
          (i32.add (i32.const 48) (i32.rem_u (call $random (i32.const 1000)) (i32.const 10))))
        (call $post (i32.const 1) (i32.const 0) (i32.const 11))))))
