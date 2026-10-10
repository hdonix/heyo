#!/bin/sh
# Runs a small Qwen model with vLLM in Docker, as an OpenAI-compatible server
# for Heyo's AI features (Settings → Advanced → AI → Local Model).
#
#   scripts/local-model.sh start|stop|status|logs
#
# HEYO_LLM_MODEL    model on Hugging Face (default Qwen/Qwen3-4B-Instruct-2507)
# HEYO_LLM_PORT     port on 127.0.0.1 (default 8000)
# HEYO_LLM_MEMORY   share of GPU memory vLLM may take (default 0.12). vLLM
#                    claims it all up front; on machines where the GPU shares
#                    memory with the system, keep this small.
# HEYO_LLM_IMAGE    vLLM image (default: NVIDIA's build when it is present,
#                    which is the one for arm64 machines, else vllm/vllm-openai)
set -eu

NAME=heyo-llm
MODEL=${HEYO_LLM_MODEL:-Qwen/Qwen3-4B-Instruct-2507}
PORT=${HEYO_LLM_PORT:-8000}
MEMORY=${HEYO_LLM_MEMORY:-0.12}
NVIDIA_IMAGE=nvcr.io/nvidia/vllm:26.03.post1-py3
if [ -n "${HEYO_LLM_IMAGE:-}" ]; then
  IMAGE=$HEYO_LLM_IMAGE
elif docker image inspect "$NVIDIA_IMAGE" >/dev/null 2>&1; then
  IMAGE=$NVIDIA_IMAGE
else
  IMAGE=vllm/vllm-openai:latest
fi

case "${1:-start}" in
  start)
    if [ -n "$(docker ps -q -f name="^$NAME\$")" ]; then
      echo "$NAME is already running on http://127.0.0.1:$PORT/v1"
      exit 0
    fi
    docker rm "$NAME" >/dev/null 2>&1 || true
    # A copy downloaded beforehand is used as is, e.g. from
    #   hf download Qwen/Qwen3-4B-Instruct-2507 --local-dir ~/.cache/heyo/models/Qwen3-4B-Instruct-2507
    # Otherwise vLLM downloads the model itself on first start.
    LOCAL="$HOME/.cache/heyo/models/$(basename "$MODEL")"
    if [ -f "$LOCAL/config.json" ]; then
      SOURCE=/model
      MOUNT="$LOCAL:/model:ro"
    else
      SOURCE=$MODEL
      mkdir -p "$HOME/.cache/huggingface"
      MOUNT="$HOME/.cache/huggingface:/root/.cache/huggingface"
    fi
    # Only reachable from this computer.
    docker run -d --name "$NAME" --gpus all --ipc=host \
      -p "127.0.0.1:$PORT:8000" \
      -v "$MOUNT" \
      --entrypoint vllm "$IMAGE" serve "$SOURCE" \
      --host 0.0.0.0 --port 8000 \
      --served-model-name "$MODEL" \
      --gpu-memory-utilization "$MEMORY" \
      --max-model-len 16384 >/dev/null
    echo "Starting $MODEL on http://127.0.0.1:$PORT/v1 — follow with: $0 logs"
    ;;
  stop)
    docker rm -f "$NAME" >/dev/null 2>&1 && echo "Stopped." || echo "Not running."
    ;;
  status)
    if curl -fsS -m 3 "http://127.0.0.1:$PORT/v1/models" >/dev/null 2>&1; then
      echo "Ready on http://127.0.0.1:$PORT/v1"
    elif [ -n "$(docker ps -q -f name="^$NAME\$")" ]; then
      echo "Starting (downloading or loading the model)…"
    else
      echo "Not running."
    fi
    ;;
  logs)
    docker logs -f --tail 40 "$NAME"
    ;;
  *)
    echo "usage: $0 start|stop|status|logs" >&2
    exit 2
    ;;
esac
