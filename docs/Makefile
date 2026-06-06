MAIN=main

all: $(MAIN).pdf

$(MAIN).pdf: $(MAIN).tex references.bib macros.tex sections/*.tex
	pdflatex $(MAIN).tex
	pdflatex $(MAIN).tex

clean:
	rm -f $(MAIN).aux $(MAIN).bbl $(MAIN).blg $(MAIN).log $(MAIN).out $(MAIN).toc $(MAIN).pdf
