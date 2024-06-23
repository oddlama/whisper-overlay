FROM nvidia/cuda:12.4.1-runtime-ubuntu22.04 as gpu

WORKDIR /app

RUN apt-get update -y && \
  apt-get install -y git python3 python3-pip libcudnn8 libcudnn8-dev libcublas-12-4 portaudio19-dev

RUN pip3 install torch==2.3.0 torchaudio==2.3.0

RUN git clone https://github.com/oddlama/RealtimeSTT
RUN pip3 install -r RealtimeSTT/requirements-gpu.txt
RUN cp -va RealtimeSTT/RealtimeSTT /app
COPY realtime-stt-server.py /app/realtime-stt-server.py

EXPOSE 7007
ENV PYTHONPATH "${PYTHONPATH}:/app"
RUN export PYTHONPATH="${PYTHONPATH}:/app"
CMD ["python3", "realtime-stt-server.py", "--host", "0.0.0.0"]
