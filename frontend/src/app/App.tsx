import {
  Alert,
  Box,
  Button,
  Center,
  MantineProvider,
  Skeleton,
  Stack,
  Text,
  Title,
} from "@mantine/core";
import { ArrowsClockwise, WarningCircle } from "@phosphor-icons/react";
import { lazy, Suspense, useState } from "react";

import { FoldersWorkspace } from "../features/folders/FoldersWorkspace";
import {
  DesktopDataProvider,
  useDesktopData,
} from "../shared/ipc/DesktopDataProvider";
import { I18nProvider, useI18n } from "../shared/i18n/I18nProvider";
import { foldryTheme } from "../shared/theme/theme";
import classes from "./App.module.css";
import { AppHeader, type AppRoute } from "./AppHeader";

const ProfilesWorkspace = lazy(() =>
  import("../features/profiles/ProfilesWorkspace").then((module) => ({
    default: module.ProfilesWorkspace,
  })),
);

function FoldryApplication() {
  const { snapshot, connection, error, preview, reload } = useDesktopData();
  const { t } = useI18n();
  const [route, setRoute] = useState<AppRoute>("folders");

  if (!snapshot && connection === "loading") {
    return <LoadingWorkspace />;
  }

  if (!snapshot) {
    return (
      <Center className={classes.fullScreen}>
        <Alert
          className={classes.errorState}
          color="red"
          icon={<WarningCircle aria-hidden size={22} />}
          title={t("offline")}
        >
          <Stack gap="md">
            <Text size="sm">{error?.message}</Text>
            <Button
              leftSection={<ArrowsClockwise aria-hidden size={17} />}
              onClick={() => void reload()}
              variant="light"
            >
              {t("retry")}
            </Button>
          </Stack>
        </Alert>
      </Center>
    );
  }

  return (
    <Box className={classes.application}>
      <AppHeader
        activeRoute={route}
        connection={connection}
        preview={preview}
        onRetry={() => void reload()}
        onRouteChange={setRoute}
      />
      {route === "folders" ? (
        <FoldersWorkspace snapshot={snapshot} />
      ) : (
        <Suspense fallback={<LoadingWorkspace />}>
          <ProfilesWorkspace snapshot={snapshot} />
        </Suspense>
      )}
    </Box>
  );
}

function LoadingWorkspace() {
  const { t } = useI18n();

  return (
    <Box className={classes.application}>
      <Box className={classes.loadingHeader}>
        <Skeleton height={30} radius="sm" width={128} />
        <Skeleton height={34} radius="sm" width={190} />
      </Box>
      <Center className={classes.loadingBody}>
        <Stack align="center" gap="sm">
          <Skeleton circle height={42} />
          <Title order={1}>{t("loadingWorkspace")}</Title>
          <Text c="dimmed">{t("loadingHint")}</Text>
        </Stack>
      </Center>
    </Box>
  );
}

export function App() {
  return (
    <MantineProvider defaultColorScheme="auto" theme={foldryTheme}>
      <I18nProvider>
        <DesktopDataProvider>
          <FoldryApplication />
        </DesktopDataProvider>
      </I18nProvider>
    </MantineProvider>
  );
}
