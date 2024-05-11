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
generateImg = ImageRGB8 (generateImage originalFnc 1200 1200)

originalFnc :: Int -> Int -> PixelRGB8
originalFnc x y =
  let (q, r) = x `quotRem` max 10 y
      s      = fromIntegral . min 0xff
  in PixelRGB8 (s q) (s r) (s (q + r + 30))
