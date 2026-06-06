# Secured Contract Spaces for MeTTa/PeTTa DeFi

This folder contains a LaTeX project for the draft specification **Secured Contract Spaces for MeTTa/PeTTa DeFi**.

## Files

- `main.tex` — top-level LaTeX document.
- `macros.tex` — packages, formatting, and shared macros.
- `references.bib` — bibliography entries.
- `sections/*.tex` — specification sections.
- `Makefile` — convenience build commands.

## Build

With a standard TeX installation:

```bash
make
```

or manually:

```bash
pdflatex main.tex
pdflatex main.tex
```

To remove build artifacts:

```bash
make clean
```
