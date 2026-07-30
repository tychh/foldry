import {
  Anchor,
  Badge,
  Box,
  Divider,
  Group,
  Modal,
  Paper,
  SimpleGrid,
  Stack,
  Text,
  ThemeIcon,
  Title,
  useComputedColorScheme,
} from "@mantine/core";
import {
  Archive,
  Eye,
  FolderSimple,
  FunnelSimpleX,
  GithubLogo,
  ListChecks,
} from "@phosphor-icons/react";
import { isTauri } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { ReactNode } from "react";

import frontendPackage from "../../package.json";
import { useI18n } from "../shared/i18n/I18nProvider";
import classes from "./ApplicationInfo.module.css";
import creatorAvatar from "./creator-avatar.png";

export const ABOUT_MENU_EVENT = "foldry://open-about";
export const HELP_MENU_EVENT = "foldry://open-help";

const PROJECT_URL = "https://github.com/tychh/foldry";
const KO_FI_URL = "https://ko-fi.com/tychh";
const CORE_STACK = [
  "Rust",
  "Tauri",
  "React",
  "TypeScript",
  "Mantine",
  "CodeMirror",
  "SQLite",
] as const;
const THIRD_PARTY =
  "Tauri · React · Mantine · Phosphor Icons · CodeMirror · rusqlite / SQLite · ignore · serde · zip · tar · flate2 · zstd";

export type InformationDialog = "about" | "help";

type ApplicationInfoProps = {
  dialog: InformationDialog | null;
  onClose: () => void;
};

export function ApplicationInfo({ dialog, onClose }: ApplicationInfoProps) {
  return (
    <>
      <AboutDialog opened={dialog === "about"} onClose={onClose} />
      <HelpDialog opened={dialog === "help"} onClose={onClose} />
    </>
  );
}

function AboutDialog({
  opened,
  onClose,
}: {
  opened: boolean;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const colorScheme = useComputedColorScheme("light");

  return (
    <Modal
      centered
      classNames={{ body: classes.modalBody, content: classes.modalContent }}
      closeButtonProps={{ "aria-label": t("aboutClose") }}
      opened={opened}
      overlayProps={{ backgroundOpacity: 0.42, blur: 2 }}
      size="46.5rem"
      title={t("aboutDialogTitle")}
      onClose={onClose}
    >
      <Stack gap="lg">
        <Group align="flex-start" gap="md" wrap="nowrap">
          <img
            alt=""
            aria-hidden
            className={classes.appIcon}
            src={
              colorScheme === "dark"
                ? "/app-icon-dark.png"
                : "/app-icon-light.png"
            }
          />
          <Box>
            <Group gap="sm">
              <Title order={2}>Foldry</Title>
              <Badge
                aria-label={t("aboutVersion")}
                color="gray"
                variant="light"
              >
                v{frontendPackage.version}
              </Badge>
            </Group>
            <Text c="dimmed" className={classes.summary} mt={6}>
              {t("aboutSummary")}
            </Text>
          </Box>
        </Group>

        <div className={classes.facts}>
          <Paper
            className={`${classes.factCard} ${classes.projectFact}`}
            p="md"
            withBorder
          >
            <Text c="dimmed" size="xs">
              {t("aboutProject")}
            </Text>
            <Anchor
              className={classes.projectLink}
              href={PROJECT_URL}
              mt={5}
              rel="noreferrer"
              target="_blank"
              onClick={(event) => {
                event.preventDefault();
                void openExternalPage(PROJECT_URL);
              }}
            >
              <GithubLogo aria-hidden size={17} weight="fill" />
              <span>tychh/foldry</span>
            </Anchor>
          </Paper>

          <Paper
            className={`${classes.factCard} ${classes.licenseFact}`}
            p="md"
            withBorder
          >
            <Text c="dimmed" size="xs">
              {t("aboutLicense")}
            </Text>
            <Text fw={650} mt={5} size="sm">
              MIT OR Apache-2.0
            </Text>
            <Text c="dimmed" mt={3} size="xs">
              {t("aboutLicenseHint")}
            </Text>
          </Paper>
        </div>

        <Box>
          <Text fw={650} size="sm">
            {t("aboutStack")}
          </Text>
          <Group gap={7} mt="xs">
            {CORE_STACK.map((item) => (
              <Badge
                className={classes.stackBadge}
                color="gray"
                key={item}
                variant="outline"
              >
                {item}
              </Badge>
            ))}
          </Group>
        </Box>

        <Box>
          <Text fw={650} size="sm">
            {t("aboutThirdParty")}
          </Text>
          <Text c="dimmed" className={classes.thirdParty} mt={5} size="xs">
            {THIRD_PARTY}
          </Text>
          <Text c="dimmed" mt={5} size="xs">
            {t("aboutThirdPartyHint")}
          </Text>
        </Box>

        <Divider />

        <Group
          align="center"
          className={classes.aboutFooter}
          justify="space-between"
        >
          <Group gap="sm" wrap="nowrap">
            <img
              alt="tychh"
              className={classes.creatorAvatar}
              src={creatorAvatar}
            />
            <Text c="dimmed" className={classes.creatorCredit} size="xs">
              {t("aboutCreatedBy")}{" "}
              <span className={classes.signature}>tychh</span>
            </Text>
          </Group>

          <Anchor
            className={classes.supportLink}
            href={KO_FI_URL}
            rel="noreferrer"
            target="_blank"
            onClick={(event) => {
              event.preventDefault();
              void openExternalPage(KO_FI_URL);
            }}
          >
            <KoFiMark />
            <span>{t("aboutSupport")}</span>
          </Anchor>
        </Group>
      </Stack>
    </Modal>
  );
}

function HelpDialog({
  opened,
  onClose,
}: {
  opened: boolean;
  onClose: () => void;
}) {
  const { t } = useI18n();

  return (
    <Modal
      centered
      classNames={{ body: classes.modalBody, content: classes.modalContent }}
      closeButtonProps={{ "aria-label": t("helpClose") }}
      opened={opened}
      overlayProps={{ backgroundOpacity: 0.42, blur: 2 }}
      size="46.5rem"
      title={t("helpDialogTitle")}
      onClose={onClose}
    >
      <Stack gap="lg">
        <Box>
          <Text className={classes.helpIntro}>{t("helpIntro")}</Text>
          <Paper className={classes.localNote} mt="md" p="sm" withBorder>
            <Text size="sm">{t("helpLocal")}</Text>
          </Paper>
        </Box>

        <SimpleGrid cols={{ base: 1, sm: 2 }} spacing="sm">
          <HelpFeature
            icon={<FolderSimple aria-hidden size={19} />}
            text={t("helpFoldersText")}
            title={t("helpFoldersTitle")}
          />
          <HelpFeature
            icon={<FunnelSimpleX aria-hidden size={19} />}
            text={t("helpProfilesText")}
            title={t("helpProfilesTitle")}
          />
          <HelpFeature
            icon={<Eye aria-hidden size={19} />}
            text={t("helpPreviewText")}
            title={t("helpPreviewTitle")}
          />
          <HelpFeature
            icon={<Archive aria-hidden size={19} />}
            text={t("helpArchiveText")}
            title={t("helpArchiveTitle")}
          />
          <HelpFeature
            className={classes.wideFeature}
            icon={<ListChecks aria-hidden size={19} />}
            text={t("helpQueueText")}
            title={t("helpQueueTitle")}
          />
        </SimpleGrid>

        <Paper className={classes.workflow} p="md" withBorder>
          <Text fw={650} size="sm">
            {t("helpWorkflowTitle")}
          </Text>
          <Text c="dimmed" mt={5} size="sm">
            {t("helpWorkflow")}
          </Text>
        </Paper>
      </Stack>
    </Modal>
  );
}

function HelpFeature({
  className,
  icon,
  text,
  title,
}: {
  className?: string;
  icon: ReactNode;
  text: string;
  title: string;
}) {
  return (
    <Paper
      className={`${classes.helpFeature} ${className ?? ""}`}
      p="md"
      withBorder
    >
      <Group align="flex-start" gap="sm" wrap="nowrap">
        <ThemeIcon color="blue" radius="md" size="lg" variant="light">
          {icon}
        </ThemeIcon>
        <Box>
          <Text fw={650} size="sm">
            {title}
          </Text>
          <Text c="dimmed" mt={4} size="sm">
            {text}
          </Text>
        </Box>
      </Group>
    </Paper>
  );
}

function KoFiMark() {
  return (
    <svg
      aria-hidden
      className={classes.koFiMark}
      viewBox="0 0 32 32"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path d="M31.844 11.932c-1.032-5.448-6.48-6.125-6.48-6.125H.964C.156 5.807.057 6.87.057 6.87S-.052 16.637.03 22.637c.22 3.228 3.448 3.561 3.448 3.561s11.021-.031 15.953-.067c3.251-.568 3.579-3.423 3.541-4.98 5.808.323 9.896-3.776 8.871-9.219Zm-14.751 4.683c-1.661 1.932-5.348 5.297-5.348 5.297s-.161.161-.417.031c-.099-.073-.14-.12-.14-.12-.595-.588-4.491-4.063-5.381-5.271-.943-1.287-1.385-3.599-.119-4.948 1.265-1.344 4.005-1.448 5.817.541 0 0 2.083-2.375 4.625-1.281 2.536 1.095 2.443 4.016.963 5.751Zm8.23.636c-1.24.156-2.244.036-2.244.036V9.714h2.359s2.631.735 2.631 3.516c0 2.552-1.313 3.557-2.745 4.021Z" />
    </svg>
  );
}

async function openExternalPage(url: string): Promise<void> {
  if (isTauri()) {
    await openUrl(url);
    return;
  }
  window.open(url, "_blank", "noopener,noreferrer");
}
