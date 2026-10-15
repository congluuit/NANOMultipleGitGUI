import { openRef } from "./actions";
import { primaryRemote } from "./model";
import { useHostedRemotes } from "./queries";
import { SafeText } from "./SafeText";

/**
 * A commit message with `#123` (and GitLab `!12`) turned into links that open the issue / pull
 * request of the repository's main GitHub / GitLab remote; plain text when there is none.
 */
export function CommitMessageText({
  repoId,
  text,
  className,
}: {
  repoId: number;
  text: string;
  className?: string;
}) {
  const remotes = useHostedRemotes(repoId);
  const remote = primaryRemote(remotes.data);
  const accountId = remote?.hasToken ? remote.accountId : null;
  return (
    <SafeText
      text={text}
      family={remote?.family ?? null}
      className={className}
      onRef={
        remote && accountId
          ? (part) =>
              void openRef(
                { accountId, fullName: remote.fullName, family: remote.family, repoId },
                part,
              )
          : undefined
      }
    />
  );
}
