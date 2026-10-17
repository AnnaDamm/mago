<?php

final class Issue2505ImmediateCallbackFailure extends Exception
{
}

function issue_2505_identity(int $value): int
{
    return $value;
}

/** @param Closure(): int $callback */
function invoke_and_assign_callback_result(Closure $callback): void
{
    try {
        $result = $callback();
        $result = issue_2505_identity($callback());
    } catch (Issue2505ImmediateCallbackFailure) {
    }
}

/** @param Closure(): int $callback */
function invoke_and_return_callback_result(Closure $callback): int
{
    try {
        return $callback();
    } catch (Issue2505ImmediateCallbackFailure) {
        return 0;
    }
}

/** @param Closure(): int $callback */
function invoke_and_return_wrapped_callback_result(Closure $callback): int
{
    try {
        return issue_2505_identity($callback());
    } catch (Issue2505ImmediateCallbackFailure) {
        return 0;
    }
}

invoke_and_assign_callback_result(/** @throws Issue2505ImmediateCallbackFailure */ static function (): int {
    throw new Issue2505ImmediateCallbackFailure();
});

invoke_and_return_callback_result(/** @throws Issue2505ImmediateCallbackFailure */ static function (): int {
    throw new Issue2505ImmediateCallbackFailure();
});

invoke_and_return_wrapped_callback_result(/** @throws Issue2505ImmediateCallbackFailure */ static function (): int {
    throw new Issue2505ImmediateCallbackFailure();
});
