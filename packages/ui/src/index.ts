// The popover UI: React components that render Everyport protocol data through a
// EveryportClient. No Tauri imports here, so Pane and other hosts can embed it.
// Import "@everyport/ui/styles.css" once; the host runs Tailwind 4.
export { Popover, type PopoverProps } from "./Popover.tsx";
export { NotificationCard, alertText } from "./views/NotificationCard.tsx";
export { ServerList } from "./views/ServerList.tsx";
export { ServerDetail } from "./views/ServerDetail.tsx";
export { CleanUp, cleanUpCandidates } from "./views/CleanUp.tsx";
export { MachineSwitcher, MachineStatus } from "./views/MachineSwitcher.tsx";
export { useViewContext, type ViewContext, type Pending } from "./context.ts";
export { Themed, themeList, warningColor, DEFAULT_THEME, type Appearance, type ThemeProps } from "./theme.tsx";
export { Socket } from "./icons.tsx";
