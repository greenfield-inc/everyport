import type { Alert, Server } from "@everyport/protocol";
import { total } from "../format.ts";
import { Socket } from "../icons.tsx";
import { DEFAULT_ALERT_MEMORY, growth, historySpan, reasonText } from "../model.ts";
import { type ThemeProps, Themed } from "../theme.tsx";

type Props = ThemeProps & {
  server: Server;
  alert: Alert;
  alertMemory?: number;
  onDetails: () => void;
  onStop: () => void;
  onSnooze: () => void;
};

/** What the alert says, such as ":6006 design-system is leaking", and one sentence of detail. */
export function alertText(server: Server, alert: Alert, alertMemory = DEFAULT_ALERT_MEMORY) {
  const who = server.project.framework ?? server.project.name;
  const using = total(alert.memory);
  if (alert.kind === "clean_up" && server.clean_up) {
    return {
      title: `:${alert.port} ${server.project.name} can be stopped`,
      body: `${reasonText(server.clean_up, server, Date.now())}. Stopping it frees ${using}.`,
    };
  }
  if (alert.kind === "leaking") {
    const minutes = Math.max(1, Math.round(historySpan(server) / 60));
    return {
      title: `:${alert.port} ${server.project.name} is leaking`,
      body: `${who} grew ${total(growth(server))} in ${minutes} ${minutes === 1 ? "minute" : "minutes"} and is now using ${using}.`,
    };
  }
  return {
    title: `:${alert.port} ${server.project.name} is over ${total(alertMemory)}`,
    body: `${who} is using ${using}.`,
  };
}

/**
 * The memory alert, for hosts that draw their own notification: Details,
 * Stop and Snooze 1h. Renders its own themed root.
 */
export function NotificationCard({ server, alert, alertMemory, onDetails, onStop, onSnooze, theme, appearance }: Props) {
  const { title, body } = alertText(server, alert, alertMemory);
  const action = "everyport:flex everyport:flex-1 everyport:justify-center everyport:rounded-lg everyport:bg-accent everyport:py-[5px] everyport:text-13 everyport:font-medium";
  return (
    <Themed theme={theme} appearance={appearance}>
      <div
        role="alert"
        className="everyport:flex everyport:w-[356px] everyport:flex-col everyport:gap-2.5 everyport:rounded-[22px] everyport:py-3 everyport:pr-3.5 everyport:pl-3"
        style={{
          background: "color-mix(in oklab, var(--popover) var(--everyport-tint), transparent)",
          boxShadow: "inset 0 0 0 0.5px var(--border), var(--everyport-shadow, 0 12px 40px rgb(0 0 0 / 0.45))",
        }}
      >
        <div className="everyport:flex everyport:items-start everyport:gap-2.5">
          <span className="everyport:flex everyport:size-[34px] everyport:shrink-0 everyport:items-center everyport:justify-center everyport:rounded-[9px] everyport:bg-[#15171D] everyport:text-white everyport:shadow-[inset_0_0_0_0.5px_rgb(255_255_255/0.18)]">
            <Socket size={24} state="running" />
          </span>
          <div className="everyport:flex everyport:min-w-0 everyport:flex-1 everyport:flex-col everyport:gap-px">
            <div className="everyport:flex everyport:items-baseline everyport:justify-between everyport:gap-2">
              <span className="everyport:clamp-1 everyport:text-13 everyport:font-medium everyport:text-fg">{title}</span>
              <span className="everyport:shrink-0 everyport:text-11 everyport:text-fg3">now</span>
            </div>
            <p className="everyport:text-13 everyport:leading-[18px] everyport:text-fg2">{body}</p>
          </div>
        </div>
        <div className="everyport:flex everyport:gap-1.5 everyport:pl-11">
          <button type="button" onClick={onDetails} className={`${action} everyport:text-fg`}>
            Details
          </button>
          <button type="button" onClick={onStop} className={`${action} everyport:text-danger`}>
            Stop
          </button>
          <button type="button" onClick={onSnooze} className={`${action} everyport:text-fg`}>
            Snooze 1h
          </button>
        </div>
      </div>
    </Themed>
  );
}
