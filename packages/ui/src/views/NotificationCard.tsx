import type { Alert, Server } from "@ppm/protocol";
import { total } from "../format.ts";
import { DotGrid } from "../icons.tsx";
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
  const action = "ppm:flex ppm:flex-1 ppm:justify-center ppm:rounded-lg ppm:bg-accent ppm:py-[5px] ppm:text-13 ppm:font-medium";
  return (
    <Themed theme={theme} appearance={appearance}>
      <div
        role="alert"
        className="ppm:flex ppm:w-[356px] ppm:flex-col ppm:gap-2.5 ppm:rounded-[22px] ppm:py-3 ppm:pr-3.5 ppm:pl-3"
        style={{
          background: "color-mix(in oklab, var(--popover) var(--ppm-tint), transparent)",
          boxShadow: "inset 0 0 0 0.5px var(--border), var(--ppm-shadow, 0 12px 40px rgb(0 0 0 / 0.45))",
        }}
      >
        <div className="ppm:flex ppm:items-start ppm:gap-2.5">
          <span className="ppm:flex ppm:size-[34px] ppm:shrink-0 ppm:items-center ppm:justify-center ppm:rounded-[9px] ppm:bg-[#15171D] ppm:text-white ppm:shadow-[inset_0_0_0_0.5px_rgb(255_255_255/0.18)]">
            <DotGrid size={24} />
          </span>
          <div className="ppm:flex ppm:min-w-0 ppm:flex-1 ppm:flex-col ppm:gap-px">
            <div className="ppm:flex ppm:items-baseline ppm:justify-between ppm:gap-2">
              <span className="ppm:clamp-1 ppm:text-13 ppm:font-medium ppm:text-fg">{title}</span>
              <span className="ppm:shrink-0 ppm:text-11 ppm:text-fg3">now</span>
            </div>
            <p className="ppm:text-13 ppm:leading-[18px] ppm:text-fg2">{body}</p>
          </div>
        </div>
        <div className="ppm:flex ppm:gap-1.5 ppm:pl-11">
          <button type="button" onClick={onDetails} className={`${action} ppm:text-fg`}>
            Details
          </button>
          <button type="button" onClick={onStop} className={`${action} ppm:text-danger`}>
            Stop
          </button>
          <button type="button" onClick={onSnooze} className={`${action} ppm:text-fg`}>
            Snooze 1h
          </button>
        </div>
      </div>
    </Themed>
  );
}
