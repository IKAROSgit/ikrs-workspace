import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { OfflineBanner } from "@/components/OfflineBanner";

describe("OfflineBanner", () => {
  let originalOnLine: boolean;

  beforeEach(() => {
    originalOnLine = navigator.onLine;
  });

  afterEach(() => {
    Object.defineProperty(navigator, "onLine", {
      value: originalOnLine,
      writable: true,
      configurable: true,
    });
  });

  it("renders nothing when online", () => {
    Object.defineProperty(navigator, "onLine", {
      value: true,
      writable: true,
      configurable: true,
    });
    const { container } = render(<OfflineBanner feature="Claude" />);
    expect(container.firstChild).toBeNull();
  });

  it("renders banner with feature name when offline", () => {
    Object.defineProperty(navigator, "onLine", {
      value: false,
      writable: true,
      configurable: true,
    });
    render(<OfflineBanner feature="Gmail" />);
    expect(
      screen.getByText("You're offline. Gmail requires an internet connection.")
    ).toBeInTheDocument();
  });

  it("renders correct message for Google Calendar", () => {
    Object.defineProperty(navigator, "onLine", {
      value: false,
      writable: true,
      configurable: true,
    });
    render(<OfflineBanner feature="Google Calendar" />);
    expect(
      screen.getByText(
        "You're offline. Google Calendar requires an internet connection."
      )
    ).toBeInTheDocument();
  });

  it("renders correct message for Google Drive", () => {
    Object.defineProperty(navigator, "onLine", {
      value: false,
      writable: true,
      configurable: true,
    });
    render(<OfflineBanner feature="Google Drive" />);
    expect(
      screen.getByText(
        "You're offline. Google Drive requires an internet connection."
      )
    ).toBeInTheDocument();
  });
});
