import type {Strings} from "./i18n/en.ts";
import type {UpstreamReport} from "./state.ts";

/** A socket send and a durable receipt are different facts in every locale. */
export function deliveryLabel(report:UpstreamReport,strings:Strings):string {
  switch(report.state) {
    case "off": return strings.upstreamOff;
    case "idle": return strings.upstreamIdle;
    case "checking": return strings.upstreamChecking;
    case "unlinked": return strings.upstreamUnlinked;
    case "connecting": return strings.upstreamConnecting(report.pending);
    case "durable": return strings.upstreamConfirmed(report.confirmed,report.pending);
    case "legacy": return strings.upstreamLegacy(report.sent,report.confirmed,report.pending);
    case "paused": return strings.upstreamPaused(report.pending);
    case "retrying": return strings.upstreamRetrying(report.retryInSeconds,report.pending);
    case "refused": return strings.upstreamRefused;
    case "conflict": return strings.upstreamConflict;
    case "local-failure": return strings.upstreamLocalFailure;
    case "incompatible": return strings.upstreamIncompatible;
    default: return strings.upstreamIncompatible;
  }
}

export function recordingProblem(code:string|null,strings:Strings):string|null {
  switch(code) {
    case "local-write-failed":
    case "local-storage-unavailable": return strings.errorLocalWrite;
    case "capture-frame-too-large": return strings.errorCaptureFrame;
    case "capture-unavailable": return strings.errorCapture;
    default: return null;
  }
}
