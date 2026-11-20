// 图片兜底组件：地址为空或远程图片加载失败时展示占位，避免布局出现空白
import React, { useEffect, useState } from 'react';
import {
  Image,
  ImageStyle,
  StyleProp,
  StyleSheet,
  Text,
  View,
  ViewStyle,
} from 'react-native';
import { Icon } from './Icon';
import { colors } from '../constants/theme';

type ResizeMode = 'cover' | 'contain' | 'stretch' | 'repeat' | 'center';

interface SafeImageProps {
  uri?: string | null;
  style?: StyleProp<ImageStyle>;
  resizeMode?: ResizeMode;
  fallbackIcon?: string;
  fallbackIconSize?: number;
  fallbackLabel?: string;
}

export default function SafeImage({
  uri,
  style,
  resizeMode = 'cover',
  fallbackIcon = 'image',
  fallbackIconSize = 28,
  fallbackLabel,
}: SafeImageProps) {
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    setFailed(false);
  }, [uri]);

  const normalizedUri = typeof uri === 'string' ? uri.trim() : '';

  if (!normalizedUri || failed) {
    return (
      <View style={[style as StyleProp<ViewStyle>, styles.fallback]}>
        <Icon name={fallbackIcon} size={fallbackIconSize} color={colors['text-tertiary']} />
        {fallbackLabel ? (
          <Text style={styles.label} numberOfLines={1}>
            {fallbackLabel}
          </Text>
        ) : null}
      </View>
    );
  }

  return (
    <Image
      source={{ uri: normalizedUri }}
      style={style}
      resizeMode={resizeMode}
      onError={() => setFailed(true)}
    />
  );
}

const styles = StyleSheet.create({
  fallback: {
    backgroundColor: colors.divider,
    alignItems: 'center',
    justifyContent: 'center',
    overflow: 'hidden',
  },
  label: {
    marginTop: 4,
    fontSize: 11,
    color: colors['text-tertiary'],
  },
});
