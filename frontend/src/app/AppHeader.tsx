import {
  ActionIcon,
  Box,
  Button,
  Group,
  Menu,
  Text,
  Tooltip,
  useComputedColorScheme,
  useMantineColorScheme,
} from "@mantine/core";
import {
  ArrowsClockwise,
  CheckCircle,
  FolderSimple,
  Globe,
  Moon,
  Sun,
  UserCircle,
  WarningCircle,
} from "@phosphor-icons/react";

import type { ConnectionState } from "../shared/ipc/DesktopDataProvider";
import { useI18n } from "../shared/i18n/I18nProvider";
import classes from "./AppHeader.module.css";

export type AppRoute = "tasks" | "profiles";

type AppHeaderProps = {
  activeRoute: AppRoute;
  connection: ConnectionState;
  preview: boolean;
  onRouteChange: (route: AppRoute) => void;
  onRetry: () => void;
};

export function AppHeader({
  activeRoute,
  connection,
  preview,
  onRouteChange,
  onRetry,
}: AppHeaderProps) {
  const { locale, setLocale, t } = useI18n();
  const { toggleColorScheme } = useMantineColorScheme();
  const colorScheme = useComputedColorScheme("light");
  const isDark = colorScheme === "dark";
  const connectionLabel =
    connection === "connected"
      ? t("connected")
      : connection === "error"
        ? t("offline")
        : t("reconnecting");

  return (
    <header className={classes.header}>
      <Group className={classes.brandGroup} gap="lg" wrap="nowrap">
        <Group gap="sm" wrap="nowrap">
          <Box aria-hidden className={classes.mark}>
            <FolderSimple size={21} weight="duotone" />
          </Box>
          <Text className={classes.wordmark}>{t("appName")}</Text>
        </Group>

        <nav aria-label={t("appName")} className={classes.navigation}>
          <Button
            className={classes.navButton}
            data-active={activeRoute === "tasks" || undefined}
            leftSection={<FolderSimple aria-hidden size={18} />}
            onClick={() => onRouteChange("tasks")}
            variant="subtle"
          >
            {t("tasks")}
          </Button>
          <Button
            className={classes.navButton}
            data-active={activeRoute === "profiles" || undefined}
            leftSection={<UserCircle aria-hidden size={18} />}
            onClick={() => onRouteChange("profiles")}
            variant="subtle"
          >
            {t("profiles")}
          </Button>
        </nav>
      </Group>

      <Group gap="xs" wrap="nowrap">
        <Tooltip label={preview ? t("demoMode") : connectionLabel}>
          <Button
            aria-label={connectionLabel}
            className={classes.connection}
            color={connection === "error" ? "red" : "gray"}
            leftSection={
              connection === "connected" ? (
                <CheckCircle aria-hidden size={17} weight="fill" />
              ) : connection === "error" ? (
                <WarningCircle aria-hidden size={17} weight="fill" />
              ) : (
                <ArrowsClockwise aria-hidden size={17} />
              )
            }
            onClick={connection === "error" ? onRetry : undefined}
            variant="subtle"
          >
            {connectionLabel}
          </Button>
        </Tooltip>

        <Menu position="bottom-end" shadow="md" width={150}>
          <Menu.Target>
            <Button
              aria-label={t("language")}
              className={classes.utilityButton}
              leftSection={<Globe aria-hidden size={18} />}
              variant="subtle"
            >
              {locale.toUpperCase()}
            </Button>
          </Menu.Target>
          <Menu.Dropdown>
            <Menu.Item onClick={() => setLocale("en")}>English</Menu.Item>
            <Menu.Item onClick={() => setLocale("ru")}>Русский</Menu.Item>
          </Menu.Dropdown>
        </Menu>

        <ActionIcon
          aria-label={isDark ? t("switchLight") : t("switchDark")}
          className={classes.themeButton}
          onClick={toggleColorScheme}
          size="lg"
          variant="subtle"
        >
          {isDark ? (
            <Sun aria-hidden size={20} />
          ) : (
            <Moon aria-hidden size={20} />
          )}
        </ActionIcon>
      </Group>
    </header>
  );
}
