SPHINXOPTS    ?=
SPHINXBUILD   ?= sphinx-build
SOURCEDIR     = .
BUILDDIR      = _build

html help: %: Makefile
	@$(SPHINXBUILD) -M $@ "$(SOURCEDIR)" "$(BUILDDIR)" $(SPHINXOPTS) $(O)

favicon_sizes:=192x192
favicons: $(foreach s,$(favicon_sizes),_static/favicon-$(s).png)

_static/favicon-%.png: _static/logo.svg Makefile
	inkscape --export-area=-5:0:130:135 --export-png-compression=9 $< --export-filename=${@:%.png=%.tmp.png}
	rm -f $@
	pngcrush -rem alla -brute ${@:%.png=%.tmp.png} $@
	rm -f ${@:%.png=%.tmp.png}

.PHONY: html help favicons
