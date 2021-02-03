## Dockerfile for latexml-runner, using a fixed commit of LaTeXML
##
## The Docker Image starts the harness on all available CPUs for the container
## and by default uses TCP connections on ports 3334-3600
##
## build via:
##
## docker build --tag latexml-runner:1.0 .
##
## run example via:
##
##
## 1. threadripper 1950x, StackExchange math-to-math
##
## docker run --cpus="30.0" --memory="16g" \
## -v "$(pwd)":/workdir -w /workdir \
## latexml-runner:1.0 -i formula_latex.txt -o formula_xml.csv \
## -l formula_status.log --timeout=65 --autoflush=10000 \
## --whatsin=math --whatsout=math --format=html5 --nodefaultresources\
## --pmml --cmml --mathtex --preload=cancel.sty \
## --preload=amsmath.sty --preload=amssymb.sty

FROM rust:latest

# [OPTIONAL] LaTeX stuff first, because it's enormous and doesn't change much
# RUN set -ex && apt-get update -qq && apt-get install -qy \
#   texlive \
#   texlive-fonts-extra \
#   texlive-lang-all \
#   texlive-latex-extra \
#   texlive-science

# latexml dependencies
RUN set -ex && apt-get update -qq && apt-get install -qy \
  build-essential \
  apt-utils \
  cpanminus \
  git \
  imagemagick \
  libarchive-zip-perl \
  libdb-dev \
  libfile-which-perl \
  libimage-magick-perl \
  libimage-size-perl \
  libio-string-perl \
  libjson-xs-perl \
  libparse-recdescent-perl \
  libtext-unidecode-perl \
  liburi-perl \
  libuuid-tiny-perl \
  libwww-perl \
  libxml-libxml-perl \
  libxml-libxslt-perl \
  libxml2 libxml2-dev \
  libxslt1-dev \
  libxslt1.1 \
  make \
  perl-doc

# Get local::lib
RUN cpanm --verbose local::lib
RUN echo 'eval "$(perl -I$HOME/perl5/lib/perl5 -Mlocal::lib)"' >>~/.bashrc

# Install LaTeXML's master branch via cpanminus
RUN export HARNESS_OPTIONS=j$(grep -c ^processor /proc/cpuinfo):c
RUN mkdir -p /opt/latexml
WORKDIR /opt/latexml
ENV LATEXML_COMMIT=6684fa5054a62646294a363387d46635e99b00e3
RUN cpanm --verbose --notest --skip-installed https://github.com/brucemiller/LaTeXML/tarball/$LATEXML_COMMIT

# Enable imagemagick policy permissions for work with arXiv PDF/EPS files
RUN perl -pi.bak -e 's/rights="none" pattern="([XE]?PS\d?|PDF)"/rights="read|write" pattern="$1"/g' /etc/ImageMagick-6/policy.xml

# Install latexmls master branch via cpanminus
RUN cpanm --notest --verbose https://github.com/dginev/LaTeXML-Plugin-latexmls/tarball/master

# Install latexml-runner
RUN cargo install latexml-runner

ENTRYPOINT ["latexml_runner"]