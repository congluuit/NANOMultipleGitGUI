import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import { TrackPatternDialog } from "./TrackPatternDialog";

beforeAll(() => {
  initI18n("vi");
});

describe("TrackPatternDialog", () => {
  it("validates the pattern and submits it with the lockable flag", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    render(<TrackPatternDialog open onSubmit={onSubmit} onCancel={() => {}} />);
    const submit = screen.getByRole("button", { name: "Track" });
    expect(submit).toBeDisabled();

    const input = screen.getByLabelText("Pattern");
    await user.type(input, "#psd");
    expect(submit).toBeDisabled();
    expect(screen.getByText(/Pattern không hợp lệ/)).toBeInTheDocument();

    await user.clear(input);
    await user.type(input, "*.psd");
    await user.click(screen.getByRole("checkbox", { name: /Lockable/ }));
    await user.click(submit);
    expect(onSubmit).toHaveBeenCalledWith("*.psd", true);
  });
});
