import {
  ActionIcon,
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
  BookOpenText,
  CheckCircle,
  FolderSimple,
  FunnelSimpleX,
  Globe,
  Moon,
  QuestionMark,
  Sun,
  WarningCircle,
} from "@phosphor-icons/react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

import type { ConnectionState } from "../shared/ipc/DesktopDataProvider";
import { useI18n } from "../shared/i18n/I18nProvider";
import classes from "./AppHeader.module.css";
import {
  ABOUT_MENU_EVENT,
  ApplicationInfo,
  HELP_MENU_EVENT,
  type InformationDialog,
} from "./ApplicationInfo";

export type AppRoute = "folders" | "profiles";

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
  const [informationDialog, setInformationDialog] =
    useState<InformationDialog | null>(null);
  const connectionLabel =
    connection === "connected"
      ? t("connected")
      : connection === "error"
        ? t("offline")
        : t("reconnecting");

  useEffect(() => {
    if (!isTauri()) {
      return;
    }

    let active = true;
    let dispose: (() => void) | undefined;

    void Promise.all([
      listen(ABOUT_MENU_EVENT, () => setInformationDialog("about")),
      listen(HELP_MENU_EVENT, () => setInformationDialog("help")),
    ]).then((unlisteners) => {
      const cleanup = () => {
        unlisteners.forEach((unlisten) => unlisten());
      };

      if (active) {
        dispose = cleanup;
      } else {
        cleanup();
      }
    });

    return () => {
      active = false;
      dispose?.();
    };
  }, []);

  return (
    <>
      <header className={classes.header}>
        <Group className={classes.brandGroup} gap="lg" wrap="nowrap">
          <Group gap="sm" wrap="nowrap">
            <img
              alt=""
              aria-hidden
              className={classes.mark}
              height={32}
              src={isDark ? "/app-icon-dark.png" : "/app-icon-light.png"}
              width={32}
            />
            <Text className={classes.wordmark}>{t("appName")}</Text>
          </Group>

          <nav aria-label={t("appName")} className={classes.navigation}>
            <Button
              className={classes.navButton}
              data-active={activeRoute === "folders" || undefined}
              leftSection={<FolderSimple aria-hidden size={18} />}
              onClick={() => onRouteChange("folders")}
              variant="subtle"
            >
              {t("folders")}
            </Button>
            <Button
              className={classes.navButton}
              data-active={activeRoute === "profiles" || undefined}
              leftSection={<FunnelSimpleX aria-hidden size={18} />}
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

          <Button
            aria-label={t("about")}
            className={`${classes.utilityButton} ${classes.infoButton}`}
            leftSection={<QuestionMark aria-hidden size={18} />}
            onClick={() => setInformationDialog("about")}
            variant="subtle"
          >
            {t("about")}
          </Button>
          <Button
            aria-label={t("help")}
            className={`${classes.utilityButton} ${classes.infoButton}`}
            leftSection={<BookOpenText aria-hidden size={18} />}
            onClick={() => setInformationDialog("help")}
            variant="subtle"
          >
            {t("help")}
          </Button>

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

      <ApplicationInfo
        dialog={informationDialog}
        onClose={() => setInformationDialog(null)}
      />
    </>
  );
}
