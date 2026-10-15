import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { Splitter } from "./Splitter";

function Harness({
  side = "left",
  onChange,
}: {
  side?: "left" | "right";
  onChange?: (px: number, final: boolean) => void;
}) {
  const [value, setValue] = useState(240);
  return (
    <Splitter
      side={side}
      label="Resize"
      controls="panel"
      value={value}
      min={180}
      max={480}
      defaultValue={240}
      onChange={(px, final) => {
        setValue(px);
        onChange?.(px, final);
      }}
    />
  );
}

const handle = () => screen.getByRole("separator", { name: "Resize" });

describe("Splitter", () => {
  it("exposes the separator semantics", () => {
    render(<Harness />);
    const h = handle();
    expect(h).toHaveAttribute("aria-orientation", "vertical");
    expect(h).toHaveAttribute("aria-valuenow", "240");
    expect(h).toHaveAttribute("aria-valuemin", "180");
    expect(h).toHaveAttribute("aria-valuemax", "480");
    expect(h).toHaveAttribute("aria-controls", "panel");
    expect(h).toHaveAttribute("tabindex", "0");
  });

  it("drags with pointer events, clamps, and reports the final width once", () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);
    const h = handle();
    fireEvent.pointerDown(h, { button: 0, clientX: 300, pointerId: 1 });
    fireEvent.pointerMove(h, { clientX: 350, pointerId: 1 });
    expect(h).toHaveAttribute("aria-valuenow", "290");
    expect(onChange).toHaveBeenLastCalledWith(290, false);
    fireEvent.pointerMove(h, { clientX: 1000, pointerId: 1 });
    expect(h).toHaveAttribute("aria-valuenow", "480");
    fireEvent.pointerUp(h, { clientX: 1000, pointerId: 1 });
    expect(onChange).toHaveBeenLastCalledWith(480, true);
    expect(onChange.mock.calls.filter(([, final]) => final)).toHaveLength(1);
    // Moving without a pressed handle does nothing.
    fireEvent.pointerMove(h, { clientX: 0, pointerId: 1 });
    expect(h).toHaveAttribute("aria-valuenow", "480");
  });

  it("grows a right-hand panel when dragged left", () => {
    render(<Harness side="right" />);
    const h = handle();
    fireEvent.pointerDown(h, { button: 0, clientX: 800, pointerId: 1 });
    fireEvent.pointerMove(h, { clientX: 760, pointerId: 1 });
    fireEvent.pointerUp(h, { clientX: 760, pointerId: 1 });
    expect(h).toHaveAttribute("aria-valuenow", "280");
  });

  it("Escape during a drag restores the starting width", () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);
    const h = handle();
    fireEvent.pointerDown(h, { button: 0, clientX: 300, pointerId: 1 });
    fireEvent.pointerMove(h, { clientX: 400, pointerId: 1 });
    fireEvent.keyDown(h, { key: "Escape" });
    expect(h).toHaveAttribute("aria-valuenow", "240");
    expect(onChange).toHaveBeenLastCalledWith(240, true);
  });

  it("supports the keyboard: arrows, Shift for bigger steps, Home / End", () => {
    render(<Harness />);
    const h = handle();
    fireEvent.keyDown(h, { key: "ArrowRight" });
    expect(h).toHaveAttribute("aria-valuenow", "256");
    fireEvent.keyDown(h, { key: "ArrowLeft", shiftKey: true });
    expect(h).toHaveAttribute("aria-valuenow", "192");
    fireEvent.keyDown(h, { key: "ArrowLeft" });
    expect(h).toHaveAttribute("aria-valuenow", "180");
    fireEvent.keyDown(h, { key: "End" });
    expect(h).toHaveAttribute("aria-valuenow", "480");
    fireEvent.keyDown(h, { key: "Home" });
    expect(h).toHaveAttribute("aria-valuenow", "180");
  });

  it("arrow keys follow the handle direction on a right-hand panel", () => {
    render(<Harness side="right" />);
    fireEvent.keyDown(handle(), { key: "ArrowLeft" });
    expect(handle()).toHaveAttribute("aria-valuenow", "256");
  });

  it("double-click resets to the default width", () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);
    fireEvent.keyDown(handle(), { key: "End" });
    fireEvent.doubleClick(handle());
    expect(handle()).toHaveAttribute("aria-valuenow", "240");
    expect(onChange).toHaveBeenLastCalledWith(240, true);
  });
});
