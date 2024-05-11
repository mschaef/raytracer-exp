module Lib
    ( doRender
    ) where

import Codec.Picture (generateImage, DynamicImage(ImageRGB8), PixelRGB8(PixelRGB8), savePngImage)
import Codec.Picture.Png (writePng)

doRender :: IO ()
doRender = do
  putStrLn "doRender"
  savePngImage "render.png" (generateImg 400 400)

generateImg :: Int -> Int -> DynamicImage
generateImg w h =
  ImageRGB8 (generateImage (originalFnc w h) w h)

originalFnc :: Int -> Int -> Int -> Int -> PixelRGB8
originalFnc w h x y =
  if x > (div w 2) && y > (div h 2)
     then PixelRGB8 255 255 255
  else PixelRGB8 0 0 0
