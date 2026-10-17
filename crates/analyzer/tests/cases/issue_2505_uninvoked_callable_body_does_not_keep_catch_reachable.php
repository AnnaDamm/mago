<?php

function uninvoked_closure_body_does_not_keep_catch_reachable(callable $dynamicCallable): void
{
    try {
        $callback = static function () use ($dynamicCallable): void {
            $dynamicCallable();
        };
    } catch (LogicException | RuntimeException) {
    }
}

function direct_local_function_body_keeps_catch_reachable(callable $dynamicCallable): void
{
    try {
        /** @throws RuntimeException */
        function issue_2505_direct_local_function(callable $callback): void
        {
            $callback();
        }

        ISSUE_2505_DIRECT_LOCAL_FUNCTION($dynamicCallable);
    } catch (LogicException | RuntimeException) {
    }
}

function direct_local_method_body_keeps_catch_reachable(callable $dynamicCallable): void
{
    try {
        class Issue2505DirectLocalMethod
        {
            /** @throws RuntimeException */
            public static function invoke(callable $callback): void
            {
                $callback();
            }
        }

        Issue2505DirectLocalMethod::INVOKE($dynamicCallable);
    } catch (LogicException | RuntimeException) {
    }
}
