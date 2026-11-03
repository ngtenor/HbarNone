import { useEffect, useRef, useState } from 'react'
import jsQR from 'jsqr'

const SCAN_INTERVAL_MS = 150

type QrScannerProps = {
  prefix: string
  onScan: (text: string) => void
  onClose: () => void
}

// Reads QR codes from the camera. Frames are decoded in memory only and never stored or sent.
export function QrScanner({ prefix, onScan, onClose }: QrScannerProps) {
  const videoRef = useRef<HTMLVideoElement>(null)
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const onScanRef = useRef(onScan)
  useEffect(() => {
    onScanRef.current = onScan
  }, [onScan])
  const [message, setMessage] = useState('Starting the camera...')

  useEffect(() => {
    let stopped = false
    let stream: MediaStream | undefined
    let timer: number | undefined

    const stop = () => {
      stopped = true
      if (timer !== undefined) window.clearTimeout(timer)
      stream?.getTracks().forEach((track) => track.stop())
    }

    const scan = () => {
      if (stopped) return
      const video = videoRef.current
      const canvas = canvasRef.current
      if (video && canvas && video.videoWidth > 0) {
        canvas.width = video.videoWidth
        canvas.height = video.videoHeight
        const context = canvas.getContext('2d', { willReadFrequently: true })
        if (context) {
          context.drawImage(video, 0, 0)
          const image = context.getImageData(0, 0, canvas.width, canvas.height)
          const code = jsQR(image.data, image.width, image.height)
          if (code) {
            const text = code.data.trim()
            if (text.startsWith(prefix)) {
              stop()
              onScanRef.current(text)
              return
            }
            setMessage('That QR code is not a signer reply. Keep looking for the right one.')
          }
        }
      }
      timer = window.setTimeout(scan, SCAN_INTERVAL_MS)
    }

    const start = async () => {
      if (!navigator.mediaDevices?.getUserMedia) {
        setMessage('This system does not allow camera access. Paste the reply instead.')
        return
      }
      try {
        const opened = await navigator.mediaDevices.getUserMedia({
          video: { facingMode: 'environment' },
          audio: false,
        })
        if (stopped) {
          opened.getTracks().forEach((track) => track.stop())
          return
        }
        stream = opened
        if (videoRef.current) {
          videoRef.current.srcObject = opened
          await videoRef.current.play().catch(() => undefined)
        }
        setMessage('Hold the signer reply QR code in front of the camera.')
        scan()
      } catch {
        setMessage('The camera could not be used. Allow camera access, or paste the reply instead.')
      }
    }

    void start()
    return stop
  }, [prefix])

  return (
    <div className="qr-scanner" role="group" aria-label="Scan the signer reply">
      <video ref={videoRef} muted playsInline aria-label="Camera preview" />
      <canvas ref={canvasRef} hidden />
      <p className="field-hint" role="status">
        {message}
      </p>
      <button className="secondary-button" type="button" onClick={onClose}>
        Stop scanning
      </button>
    </div>
  )
}
