module Lib
    ( doRender
    ) where

import Codec.Picture (generateImage, DynamicImage(ImageRGB8), PixelRGB8(PixelRGB8), savePngImage)
import Codec.Picture.Png (writePng)

doRender :: IO ()
doRender = do
  putStrLn "doRender"
  savePngImage "render.png" generateImg

generateImg :: DynamicImage
generateImg = ImageRGB8 (generateImage originalFnc 200 200)

originalFnc :: Int -> Int -> PixelRGB8
originalFnc x y =
  if x > 100 && y > 100
     then PixelRGB8 255 255 255
  else PixelRGB8 0 0 0
