# 21. A weekly letter, asked for

Date: 2026-10-13

## Status

Accepted.

## Context

v0.18 turns a week of listening into a letter: what it opened, what was heard
most, where the home star stands. Until now lyrid sent two letters, both in
answer to something the person had just done (ADR 0012, v0.11), and the
charter promised exactly that: "two letters, ever ... no newsletter". A letter
that arrives on its own every week is a different kind of mail, and the kind
that ends up in spam folders and in complaints.

## Decision

- **Asked for, never assumed.** A row in `weekly_letter` is the asking; no row,
  no letter. Only a confirmed address can ask.
- **Once a week at most.** Weeks are ISO weeks in UTC, Monday to Monday --
  lyrid does not know where its readers live. A sender looks once an hour,
  takes the week with one `UPDATE` before writing the letter (two servers
  cannot both send it; a crash between the two loses a letter rather than
  doubling one) and jumps to the latest week, so a server that was down for a
  month sends one letter about last week rather than four old ones. A week
  with no listening sends nothing.
- **One click to stop, two ways.** The body links to a page that stops the
  letter with one press -- a press, not on arrival, because mail scanners open
  links before people do. The headers carry `List-Unsubscribe` and
  `List-Unsubscribe-Post` (RFC 8058) for the mail client's own button, which
  posts to the API without opening anything. Both use a random token stored
  with the asking; the answer is the same whether a letter was stopped or
  there was none.
- **In the reader's language.** The interface keeps its language per browser,
  and a letter has no browser, so the language is stored with the asking: the
  one the person was reading in when they asked. The words live in
  `src/letter/{en,ru}.json`, checked against each other the way the
  interface's are.
- **Plain text**, like the other letters.

## Consequences

- The charter changed: besides the two letters that answer an action, the only
  mail is this one, and only when asked for. The export lists the asking
  (language and date, without the token) and deleting the account removes it.
- A stand without SMTP writes the letter to its log, as it does the others.
- Changing the language means asking again from the interface in the other
  language; there is no separate setting for it.
