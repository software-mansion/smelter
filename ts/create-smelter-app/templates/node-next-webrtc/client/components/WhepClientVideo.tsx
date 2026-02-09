"use client"

import { WhepClient } from '@/utils/whep-client';
import React, { useEffect, useRef } from 'react';

type VideoProps = React.DetailedHTMLProps<
  React.VideoHTMLAttributes<HTMLVideoElement>,
  HTMLVideoElement
>;

type SmelterVideoProps = {
  url: string,
  bearerToken?: string,
} & VideoProps;

export default function WhepClientVideo(props: SmelterVideoProps) {
  const { url, bearerToken, ...videoElementProps } = props;

  const videoRef = useRef<HTMLVideoElement>(null);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) {
      return;
    }

    const client = new WhepClient()
    const promise = (async () => {
      const stream = await client.connect(url, bearerToken);
      if (stream && videoRef.current) {
        video.srcObject = stream;
        try {
          await video.play();
        } catch (err) {
          console.log("Failed to auto start player", err)
          // if auto play is blocked by browser
          video.muted = true;
          await video.play();
        }
      }
    })()

    return () => {
      void promise
        .catch(console.error)
        .then(() => client.close)
        .catch(console.error);
    };
  }, [url, bearerToken]);

  return <video ref={videoRef} {...videoElementProps} />;
}
