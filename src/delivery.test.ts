import assert from "node:assert/strict";
import {test} from "node:test";
import {en} from "./i18n/en.ts";
import {deliveryLabel, recordingProblem} from "./delivery.ts";
import type {UpstreamReport} from "./state.ts";

const report=(state:UpstreamReport["state"]):UpstreamReport=>({state,sent:12,confirmed:3,pending:7,retryInSeconds:4,failures:1});

test("durable delivery reports confirmed frames rather than socket sends",()=>{
  assert.equal(deliveryLabel(report("durable"),en),en.upstreamConfirmed(3,7));
});

test("legacy sends retain their unconfirmed local status",()=>{
  assert.equal(deliveryLabel(report("legacy"),en),en.upstreamLegacy(12,3,7));
  assert.equal(deliveryLabel(report("paused"),en),en.upstreamPaused(7));
});

test("refusal conflict and local failure remain readable without hiding retention",()=>{
  assert.equal(deliveryLabel(report("refused"),en),en.upstreamRefused);
  assert.equal(deliveryLabel(report("conflict"),en),en.upstreamConflict);
  assert.equal(deliveryLabel(report("local-failure"),en),en.upstreamLocalFailure);
  assert.equal(recordingProblem("local-write-failed",en),en.errorLocalWrite);
  assert.equal(recordingProblem("capture-frame-too-large",en),en.errorCaptureFrame);
  assert.equal(recordingProblem(null,en),null);
});
