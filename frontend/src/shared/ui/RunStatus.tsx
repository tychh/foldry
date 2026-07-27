import { Box, Text } from "@mantine/core";

import type { RunState } from "../contracts/generated";
import { type MessageKey, useI18n } from "../i18n/I18nProvider";
import classes from "./RunStatus.module.css";

type DisplayRunState = RunState | "ready";

const stateMessage: Record<DisplayRunState, MessageKey> = {
  ready: "ready",
  queued: "queued",
  planning: "planning",
  running: "running",
  paused: "paused",
  stopping: "stopping",
  succeeded: "succeeded",
  succeeded_with_warnings: "warnings",
  failed: "failed",
  stopped: "stopped",
  interrupted: "interrupted",
};

export function RunStatus({ state }: { state: DisplayRunState }) {
  const { t } = useI18n();

  return (
    <Box className={classes.root} data-state={state}>
      <Box aria-hidden className={classes.dot} />
      <Text component="span" fw={600} size="sm">
        {t(stateMessage[state])}
      </Text>
    </Box>
  );
}
